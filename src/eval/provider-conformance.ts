import {readFile} from "node:fs/promises";
import {resolve} from "node:path";
import {fileURLToPath} from "node:url";
import type {ModelToolDefinition} from "../agent/model-action.js";
import {BuiltinProviderAdapter} from "../providers/builtin-provider-adapter.js";
import type {HttpStreamRequest, HttpStreamTransport} from "../providers/http-transport.js";
import {parseModelProfile} from "../providers/model-profile.js";
import {ProviderError} from "../providers/provider-error.js";
import {StrictProviderGateway} from "../providers/provider-gateway.js";
import {parseProviderProfile} from "../providers/provider-profile.js";
import type {ApiProtocol} from "../types.js";

interface ConformanceFixture {schemaVersion: 1; protocol: ApiProtocol; successFrames: unknown[]; toolFrames: unknown[]; failureFrames: unknown[]; prematureFrames: unknown[]; malformedPayload: string}
export interface ProviderConformanceReport {readonly schemaVersion: 1; readonly protocols: readonly {protocol: ApiProtocol; checks: readonly {name: string; passed: boolean}[]; passed: boolean}[]; readonly passed: boolean}

const FEATURES = Object.freeze({streaming: true, native_tool_calls: true, parallel_tool_calls: true, structured_output: false, reasoning_metadata: true, usage: true, prompt_caching: false, image_input: false, audio_input: false, provider_hosted_tools: false});
const TOOL: ModelToolDefinition = Object.freeze({name: "invoke_local_capability", description: "Invoke a local capability", inputSchema: Object.freeze({type: "object"})});

export async function runProviderConformanceReport(fixtureRoot = fileURLToPath(new URL("../../test/fixtures/providers/conformance/", import.meta.url))): Promise<ProviderConformanceReport> {
  const protocols = [];
  for (const protocol of ["responses", "chat_completions"] as const) {
    const fixture = JSON.parse(await readFile(resolve(fixtureRoot, `${protocol}.json`), "utf8")) as ConformanceFixture;
    if (fixture.schemaVersion !== 1 || fixture.protocol !== protocol) throw new Error(`Invalid ${protocol} conformance fixture.`);
    const checks = await runProtocol(protocol, fixture);
    protocols.push(Object.freeze({protocol, checks: Object.freeze(checks), passed: checks.every((check) => check.passed)}));
  }
  return Object.freeze({schemaVersion: 1, protocols: Object.freeze(protocols), passed: protocols.every((item) => item.passed)});
}

async function runProtocol(protocol: ApiProtocol, fixture: ConformanceFixture): Promise<{name: string; passed: boolean}[]> {
  const checks: {name: string; passed: boolean}[] = [];
  const successTransport = new StaticTransport(sse(fixture.successFrames));
  const success = await collect(new StrictProviderGateway(successTransport), protocol);
  const request = successTransport.request;
  checks.push(check("request_validation", Boolean(request && request.url.endsWith(protocol === "responses" ? "/responses" : "/chat/completions") && request.headers.Authorization === "Bearer fixture-secret" && request.body.model === "fixture-model")));
  checks.push(check("stream_usage_completion", success.some((item) => item.type === "text.delta") && success.some((item) => item.type === "usage") && success.at(-1)?.type === "completed"));
  checks.push(check("reasoning_redaction", !JSON.stringify(success).includes("PRIVATE_REASONING") && !JSON.stringify(success).includes("fixture-secret")));

  const toolTransport = new StaticTransport(sse(fixture.toolFrames));
  const tools = await collect(new StrictProviderGateway(toolTransport), protocol, [TOOL]);
  checks.push(check("tool_calls", tools.some((item) => item.type === "tool.call" && item.call.callId === "call-1" && item.call.argumentsJson.includes("README.md")) && JSON.stringify(toolTransport.request?.body).includes(TOOL.name)));
  checks.push(check("malformed_eof", await rejectsProtocol(protocol, ssePayload(fixture.malformedPayload)) && await rejectsProtocol(protocol, sse(fixture.prematureFrames))));
  checks.push(check("failure_redaction", await rejectsRedactedFailure(protocol, sse(fixture.failureFrames))));
  checks.push(check("cancellation", await cancellationPasses(protocol)));
  checks.push(check("feature_fail_closed", await unsupportedFeatureFails(protocol)));
  return checks;
}

async function collect(gateway: StrictProviderGateway, protocol: ApiProtocol, tools?: readonly ModelToolDefinition[]) {
  const events = [];
  for await (const event of gateway.streamProfile({providerProfile: provider(protocol), modelProfile: model(protocol), apiKey: "fixture-secret", messages: [{role: "user", content: "hello"}], maxOutputTokens: 512, ...(tools ? {tools} : {})})) events.push(event);
  return events;
}

async function rejectsProtocol(protocol: ApiProtocol, response: Response): Promise<boolean> {
  try { await collect(new StrictProviderGateway(new StaticTransport(response)), protocol); return false; }
  catch (error) { return error instanceof ProviderError && error.kind === "protocol" && !error.message.includes("SECRET_FRAME"); }
}

async function rejectsRedactedFailure(protocol: ApiProtocol, response: Response): Promise<boolean> {
  try { await collect(new StrictProviderGateway(new StaticTransport(response)), protocol); return false; }
  catch (error) { return error instanceof ProviderError && error.kind === "rate_limit" && !error.message.includes("SECRET_PROVIDER_DETAIL") && !error.message.includes("fixture-secret"); }
}

async function cancellationPasses(protocol: ApiProtocol): Promise<boolean> {
  const controller = new AbortController(); controller.abort();
  const transport: HttpStreamTransport = {async postStream(request) { if (request.signal?.aborted) throw new DOMException("Aborted", "AbortError"); return new Response(); }};
  try {
    for await (const _event of new StrictProviderGateway(transport).streamProfile({providerProfile: provider(protocol), modelProfile: model(protocol), apiKey: "fixture-secret", messages: [{role: "user", content: "hello"}], maxOutputTokens: 10, signal: controller.signal})) { /* consume */ }
    return false;
  } catch (error) { return error instanceof DOMException && error.name === "AbortError"; }
}

async function unsupportedFeatureFails(protocol: ApiProtocol): Promise<boolean> {
  const p = provider(protocol);
  const unsupported = parseModelProfile({...model(protocol), featureProfile: {...model(protocol).featureProfile, features: {...FEATURES, structured_output: true}}});
  try { await new BuiltinProviderAdapter(new StaticTransport(new Response())).createRuntime({providerProfile: p, modelProfile: unsupported, credential: {targetId: "fixture", readSecret: async () => "fixture-secret"}}); return false; }
  catch { return true; }
}

function provider(protocol: ApiProtocol) {
  return parseProviderProfile({schemaVersion: 1, providerProfileId: `provider:test/${protocol}`, adapterId: "adapter:minimax/builtin", displayName: `${protocol} fixture`, enabled: true, transport: {baseUrl: "https://provider.test/v1", protocol, publicHeaders: {}, allowInsecureLoopback: false}, authentication: {kind: "bearer", envBinding: "TEST_PROVIDER_KEY"}});
}

function model(protocol: ApiProtocol) {
  return parseModelProfile({schemaVersion: 1, modelProfileId: `model:test/${protocol}/fixture-model`, providerProfileId: `provider:test/${protocol}`, displayName: "Fixture Model", model: "fixture-model", enabled: true, featureProfile: {schemaVersion: 1, features: FEATURES, contextWindow: 32_000, maxOutputTokens: 2_048}});
}

class StaticTransport implements HttpStreamTransport {
  request: HttpStreamRequest | undefined;
  constructor(private readonly response: Response) {}
  async postStream(request: HttpStreamRequest): Promise<Response> { this.request = request; return this.response; }
}

function sse(frames: readonly unknown[]): Response { return new Response(frames.map((frame) => `data: ${typeof frame === "string" ? frame : JSON.stringify(frame)}\n\n`).join(""), {status: 200, headers: {"Content-Type": "text/event-stream"}}); }
function ssePayload(payload: string): Response { return new Response(`data: ${payload}\n\n`, {status: 200, headers: {"Content-Type": "text/event-stream"}}); }
function check(name: string, passed: boolean) { return Object.freeze({name, passed}); }

async function main(): Promise<void> { const report = await runProviderConformanceReport(); console.log(JSON.stringify(report, null, 2)); if (!report.passed) process.exitCode = 1; }
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) void main();
