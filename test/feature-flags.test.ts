import assert from "node:assert/strict";
import {mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {ConfigManager, DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {DEFAULT_AGENT_FEATURE_FLAGS, parseAgentFeatureFlagConfig, resolveAgentFeatureFlags, RuntimeFeatureFlagService} from "../src/config/feature-flags.js";
import {LocalCapabilityRuntime} from "../src/capabilities/local-capability-runtime.js";

test("feature flags default off and dependency-invalid combinations fail closed", () => {
  assert.deepEqual(DEFAULT_AGENT_FEATURE_FLAGS, {capabilityCatalog: false, capabilityEmbedding: false, agentExecution: false, agentDefaultRoute: false});
  const invalid = resolveAgentFeatureFlags({capabilityCatalog: false, capabilityEmbedding: true, agentExecution: true, agentDefaultRoute: true}, {releaseGatePassed: true});
  assert.equal(invalid.capabilityCatalog, false);
  assert.equal(invalid.capabilityEmbedding, false);
  assert.equal(invalid.agentExecution, false);
  assert.equal(invalid.agentDefaultRoute, false);
  assert.ok(invalid.diagnostics.includes("embedding_requires_catalog"));
  assert.ok(invalid.diagnostics.includes("agent_requires_catalog"));
});

test("catalog, embedding, execution and default routing remain independently reversible", () => {
  const lexicalAgent = resolveAgentFeatureFlags({capabilityCatalog: true, capabilityEmbedding: false, agentExecution: true, agentDefaultRoute: false}, {releaseGatePassed: true});
  assert.deepEqual([lexicalAgent.capabilityCatalog, lexicalAgent.capabilityEmbedding, lexicalAgent.agentExecution, lexicalAgent.agentDefaultRoute], [true, false, true, false]);
  const catalogOnly = resolveAgentFeatureFlags({capabilityCatalog: true, capabilityEmbedding: true, agentExecution: false, agentDefaultRoute: false}, {releaseGatePassed: true});
  assert.deepEqual([catalogOnly.capabilityCatalog, catalogOnly.capabilityEmbedding, catalogOnly.agentExecution], [true, true, false]);
  const gated = new RuntimeFeatureFlagService(false).initialize({capabilityCatalog: true, capabilityEmbedding: false, agentExecution: true, agentDefaultRoute: true});
  assert.equal(gated.agentDefaultRoute, false);
  assert.ok(gated.diagnostics.includes("default_route_gate_failed"));
});

test("capability runtime failure disables every dependent path while preserving chat", () => {
  const service = new RuntimeFeatureFlagService(true);
  service.initialize({capabilityCatalog: true, capabilityEmbedding: true, agentExecution: true, agentDefaultRoute: true});
  const disabled = service.disableCapabilityRuntime();
  assert.deepEqual(
    [disabled.capabilityCatalog, disabled.capabilityEmbedding, disabled.agentExecution, disabled.agentDefaultRoute],
    [false, false, false, false]
  );
  assert.ok(disabled.diagnostics.includes("catalog_runtime_failed"));
});

test("feature config is strict but omitted legacy config stays omitted and byte-stable", async () => {
  assert.throws(() => parseAgentFeatureFlagConfig({agentExecution: "yes"}), /boolean/);
  assert.throws(() => parseAgentFeatureFlagConfig({unknown: true}), /unknown/);
  const root = await mkdtemp(join(tmpdir(), "feature-config-"));
  try {
    const path = join(root, "config.json");
    const raw = JSON.stringify(DEFAULT_CONFIG, null, 2) + "\n";
    await writeFile(path, raw);
    const loaded = await new ConfigManager(root).load();
    assert.equal(loaded.features, undefined);
    assert.equal(await readFile(path, "utf8"), raw);
  } finally { await rm(root, {recursive: true, force: true}); }
});

test("catalog enabled with embedding disabled still provides exact plus BM25 without network or resource files", async () => {
  const root = await mkdtemp(join(tmpdir(), "feature-catalog-"));
  try {
    const runtime = new LocalCapabilityRuntime({workspaceRoot: root, stateRoot: join(root, ".mini-codex"), userConfigRoot: join(root, "user"), getPermissionMode: () => "confirm", env: {}, homeDir: join(root, "home")});
    const flags = resolveAgentFeatureFlags({capabilityCatalog: true, capabilityEmbedding: false, agentExecution: false, agentDefaultRoute: false}, {releaseGatePassed: true});
    await runtime.initialize(flags);
    assert.equal(runtime.list().mode, "exact+bm25");
    const exact = await runtime.search("read file");
    assert.equal(exact.candidates[0]?.id, "capability:minimax/read-file");
    const lexical = await runtime.search("查看目录");
    assert.equal(lexical.candidates[0]?.id, "capability:minimax/list-files");
  } finally { await rm(root, {recursive: true, force: true}); }
});
