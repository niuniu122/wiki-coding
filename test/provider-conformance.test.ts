import {defineBuiltinProviderConformanceSuite} from "./support/provider-conformance-suite.js";
import assert from "node:assert/strict";
import test from "node:test";
import {runProviderConformanceReport} from "../src/eval/provider-conformance.js";
import {DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {BuiltinProviderAdapter} from "../src/providers/builtin-provider-adapter.js";
import {createDefaultProviderAdapterRegistry} from "../src/providers/provider-adapter-registry.js";
import {ModelProfileRegistry} from "../src/runtime/model-profile-registry.js";

defineBuiltinProviderConformanceSuite();

test("offline conformance report covers every enabled built-in model protocol", async () => {
  const report = await runProviderConformanceReport();
  assert.equal(report.passed, true);
  const passedProtocols = new Set(report.protocols.filter((item) => item.passed).map((item) => item.protocol));
  const registry = new ModelProfileRegistry(createDefaultProviderAdapterRegistry(new BuiltinProviderAdapter()));
  await registry.initialize(DEFAULT_CONFIG);
  const models = registry.listModels();
  assert.ok(models.length > 0);
  for (const entry of models) {
    assert.equal(passedProtocols.has(entry.providerProfile.transport.protocol), true, entry.modelProfile.modelProfileId);
    assert.equal(entry.modelProfile.enabled, true);
    assert.equal(entry.providerProfile.enabled, true);
  }
  assert.equal(report.protocols.every((item) => item.checks.length === 8 && item.checks.every((check) => check.passed)), true);
});
