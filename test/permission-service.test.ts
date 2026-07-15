import assert from "node:assert/strict";
import test from "node:test";
import {PermissionService} from "../src/runtime/permission-service.js";
import {classifyChatInput} from "../src/ui/chat-input-policy.js";

test("permission modes are explicit session-only state", () => {
  const permissions = new PermissionService();
  assert.equal(permissions.current, "confirm");
  assert.equal(permissions.set("workspace_read"), "workspace_read");
  assert.equal(permissions.set("full_access"), "full_access");
  permissions.resetSession();
  assert.equal(permissions.current, "confirm");
});

test("permission slash commands accept only the three named modes", () => {
  assert.deepEqual(classifyChatInput("/permissions"), {
    type: "command",
    command: {type: "permission.show"}
  });
  assert.deepEqual(classifyChatInput("/permissions workspace-read"), {
    type: "command",
    command: {type: "permission.set", mode: "workspace_read"}
  });
  assert.deepEqual(classifyChatInput("/permissions full-access"), {
    type: "command",
    command: {type: "permission.set", mode: "full_access"}
  });
  assert.equal(classifyChatInput("/permissions forever").type, "invalid");
});
