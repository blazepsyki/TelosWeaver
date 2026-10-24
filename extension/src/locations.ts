// Project identity and user-only IPC locations (Spec §3.3, G0 REQ-007).
import { createHash } from "node:crypto";
import { realpath } from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";

export function normalizeRoot(canonical: string, windows: boolean): string {
  let value = canonical;
  if (windows) {
    value = value.startsWith("\\\\?\\UNC\\")
      ? `//${value.slice(8)}`
      : value.replace(/^\\\\\?\\/, "");
    value = value.replaceAll("\\", "/").toLowerCase();
  }
  const root = value === "/" || (windows && /^[a-z]:\/$/.test(value));
  return root ? value : value.replace(/\/+$/, "");
}
export function projectKey(normalized: string): string {
  return createHash("sha256")
    .update(normalized, "utf8")
    .digest("hex")
    .slice(0, 16);
}
export async function locations(
  root: string,
  env: NodeJS.ProcessEnv = process.env,
) {
  const canonical = await realpath(root);
  const key = projectKey(
    normalizeRoot(canonical, process.platform === "win32"),
  );
  const windows = process.platform === "win32";
  const stateBase = windows
    ? env.LOCALAPPDATA
    : env.XDG_STATE_HOME || path.join(os.homedir(), ".local", "state");
  if (!stateBase || !path.isAbsolute(stateBase))
    throw new Error("사용자 상태 디렉터리를 확인할 수 없습니다.");
  const runtime =
    env.XDG_RUNTIME_DIR ||
    path.join("/tmp", `telos-weaver-${process.getuid?.()}`);
  return {
    root: canonical,
    key,
    endpoint: windows
      ? `\\\\.\\pipe\\telos-weaver-${key}`
      : env.XDG_RUNTIME_DIR
        ? path.join(runtime, "telos-weaver", `${key}.sock`)
        : path.join(runtime, `${key}.sock`),
    tokenPath: path.join(stateBase, "telos-weaver", key, "api-token"),
  };
}
export type Locations = Awaited<ReturnType<typeof locations>>;
