import assert from "node:assert/strict";
import {resolve} from "node:path";
import test from "node:test";
import {resolveUserConfigDir} from "../src/config/credential-store.js";
import {resolveUserConfigRoot} from "../src/config/user-config-root.js";

test("MINIMAX_CODEX_HOME overrides every OS user config default", () => {
  const configured = resolve("portable-minimax-home");

  for (const platform of ["win32", "darwin", "linux"] as const) {
    assert.equal(
      resolveUserConfigRoot({
        env: {MINIMAX_CODEX_HOME: `  ${configured}  `},
        platform,
        homeDir: resolve("unused-home")
      }),
      configured
    );
  }
});

test("user config root preserves the existing OS-specific defaults", () => {
  const homeDir = resolve("fixture-home");

  assert.equal(
    resolveUserConfigRoot({env: {}, platform: "win32", homeDir}),
    resolve(homeDir, "AppData", "Roaming", "minimax-codex")
  );
  assert.equal(
    resolveUserConfigRoot({
      env: {APPDATA: resolve("fixture-appdata")},
      platform: "win32",
      homeDir
    }),
    resolve("fixture-appdata", "minimax-codex")
  );
  assert.equal(
    resolveUserConfigRoot({env: {}, platform: "darwin", homeDir}),
    resolve(homeDir, "Library", "Application Support", "minimax-codex")
  );
  assert.equal(
    resolveUserConfigRoot({env: {}, platform: "linux", homeDir}),
    resolve(homeDir, ".config", "minimax-codex")
  );
  assert.equal(
    resolveUserConfigRoot({
      env: {XDG_CONFIG_HOME: resolve("fixture-xdg")},
      platform: "linux",
      homeDir
    }),
    resolve("fixture-xdg", "minimax-codex")
  );
});

test("the legacy credential resolver remains a compatibility alias", () => {
  assert.equal(resolveUserConfigDir(), resolveUserConfigRoot());
});
