import assert from "node:assert/strict";
import test from "node:test";
import {parseCapabilityManifest} from "../src/capabilities/capability-manifest.js";

const SOURCE = {
  kind: "minimax" as const,
  scope: "project_native" as const,
  root: "C:/workspace/.minimax/capabilities",
  file: "C:/workspace/.minimax/capabilities/read.json"
};

test("capability manifests are finite, normalized metadata contracts", () => {
  const parsed = parseCapabilityManifest({
    schemaVersion: 1,
    id: "capability:minimax/read-file",
    name: "Read file",
    description: "Read a text file inside the workspace",
    aliases: ["查看文件"],
    commands: ["/read"],
    safetyClass: "workspace_read",
    execution: {kind: "workspace_read", operation: "read_file"},
    facets: {domain: ["workspace"], action: ["read"], object: ["file"]}
  }, SOURCE);
  assert.equal(parsed.availability, "available");
  assert.match(parsed.intentDocument, /查看文件/);
  assert.equal(Object.isFrozen(parsed), true);
});

test("unknown authority fields and malformed execution fail closed", () => {
  for (const value of [
    {schemaVersion: 1, id: "capability:minimax/x", name: "x", description: "x", safetyClass: "catalog_read", execution: {kind: "metadata_only"}, shell: "rm -rf"},
    {schemaVersion: 2, id: "capability:minimax/x", name: "x", description: "x", safetyClass: "catalog_read", execution: {kind: "metadata_only"}},
    {schemaVersion: 1, id: "../escape", name: "x", description: "x", safetyClass: "catalog_read", execution: {kind: "metadata_only"}},
    {schemaVersion: 1, id: "capability:minimax/x", name: "x", description: "x", safetyClass: "unknown", execution: {kind: "metadata_only"}},
    {schemaVersion: 1, id: "capability:minimax/x", name: "x", description: "x", safetyClass: "local_diagnostic", execution: {kind: "npm_script", script: "test", argv: "--watch"}}
  ]) {
    assert.throws(() => parseCapabilityManifest(value, SOURCE), /Invalid capability manifest/);
  }
});
