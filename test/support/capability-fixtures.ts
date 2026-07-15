import {parseCapabilityManifest} from "../../src/capabilities/capability-manifest.js";
import type {CapabilityDescriptor} from "../../src/capabilities/types.js";

export function capabilityFixtures(): readonly CapabilityDescriptor[] {
  return Object.freeze([
    descriptor("read-file", "Read workspace file", "Read and inspect a text file in the local project 查看项目文件", ["查看文件", "show file"], ["/read"], "workspace_read", {domain: ["workspace"], action: ["read"], object: ["file"]}),
    descriptor("search-code", "Search project code", "Find text and symbols inside local source files 搜索代码", ["查找代码", "find symbol"], ["/search"], "workspace_read", {domain: ["workspace"], action: ["search"], object: ["code"]}),
    descriptor("npm-test", "Run project tests", "Run the declared npm test diagnostic 检查项目测试", ["测试项目", "check tests"], ["/test"], "local_diagnostic", {domain: ["project"], action: ["test"], object: ["npm"]})
  ]);
}

function descriptor(segment: string, name: string, description: string, aliases: string[], commands: string[], safetyClass: "workspace_read" | "local_diagnostic", facets: {domain: string[]; action: string[]; object: string[]}): CapabilityDescriptor {
  return parseCapabilityManifest({
    schemaVersion: 1,
    id: `capability:minimax/${segment}`,
    name, description, aliases, commands, safetyClass,
    execution: safetyClass === "workspace_read" ? {kind: "workspace_read", operation: "read_file"} : {kind: "npm_script", script: "test", argv: []},
    facets
  }, {kind: "minimax", scope: "builtin", root: "builtin", file: `${segment}.json`});
}
