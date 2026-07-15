import {homedir} from "node:os";
import {join, resolve} from "node:path";

export interface UserConfigRootOptions {
  readonly env?: Readonly<Record<string, string | undefined>>;
  readonly platform?: NodeJS.Platform;
  readonly homeDir?: string;
}

export function resolveUserConfigRoot(options: UserConfigRootOptions = {}): string {
  const env = options.env ?? process.env;
  const platform = options.platform ?? process.platform;
  const homeDir = options.homeDir ?? homedir();
  const override = env.MINIMAX_CODEX_HOME?.trim();

  if (override) {
    return resolve(override);
  }
  if (platform === "win32") {
    return join(env.APPDATA?.trim() || join(homeDir, "AppData", "Roaming"), "minimax-codex");
  }
  if (platform === "darwin") {
    return join(homeDir, "Library", "Application Support", "minimax-codex");
  }
  return join(env.XDG_CONFIG_HOME?.trim() || join(homeDir, ".config"), "minimax-codex");
}
