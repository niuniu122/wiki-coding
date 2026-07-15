import {parseCapabilityManifest} from "./capability-manifest.js";
import type {CapabilityDescriptor} from "./types.js";

export function builtinCapabilityDescriptors(): readonly CapabilityDescriptor[] {
  const source = {kind: "minimax" as const, scope: "builtin" as const, root: "builtin", file: "builtin"};
  return Object.freeze([
    parseCapabilityManifest({
      schemaVersion: 1,
      id: "capability:minimax/read-file",
      name: "Read workspace file",
      description: "Read one bounded UTF-8 text file inside the current workspace",
      aliases: ["read file", "查看文件", "读取文件"],
      commands: [],
      safetyClass: "workspace_read",
      idempotent: true,
      execution: {kind: "workspace_read", operation: "read_file"},
      facets: {domain: ["workspace"], action: ["read"], object: ["file"]}
    }, {...source, file: "read-file"}),
    parseCapabilityManifest({
      schemaVersion: 1,
      id: "capability:minimax/list-files",
      name: "List workspace directory",
      description: "List bounded file and directory names inside one workspace directory",
      aliases: ["list files", "查看目录", "列出文件"],
      commands: [],
      safetyClass: "workspace_read",
      idempotent: true,
      execution: {kind: "workspace_read", operation: "list_files"},
      facets: {domain: ["workspace"], action: ["list"], object: ["file", "directory"]}
    }, {...source, file: "list-files"})
  ]);
}
