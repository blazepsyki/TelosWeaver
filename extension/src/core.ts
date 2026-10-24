// Explicit Core startup and detached lifetime (Spec §3.3, §17; PH0-EXT).
import { EventEmitter } from "node:events";
import { execFile, spawn } from "node:child_process";
import { open, realpath, stat } from "node:fs/promises";
import * as path from "node:path";
import { promisify } from "node:util";
import {
  CoreClient,
  ConnectionError,
  helloResult,
  healthResult,
} from "./client";
import { locations, Locations } from "./locations";
import type { HelloResult } from "./generated/ApiHelloResult";
import type { HealthResult } from "./generated/ApiHealthResult";
import type { HelloParams } from "./generated/ApiHelloParams";

export interface CoreOptions {
  executable: string;
  projectId?: string;
  hostId?: string;
  env?: NodeJS.ProcessEnv;
  timeoutMs?: number;
}
export interface ConnectionState {
  phase: "disconnected" | "connecting" | "connected" | "error";
  root?: string;
  hello?: HelloResult;
  health?: HealthResult;
  message?: string;
}
export class CoreConnection extends EventEmitter {
  state: ConnectionState = { phase: "disconnected" };
  client: CoreClient | undefined;
  private generation = 0;
  private heartbeat: NodeJS.Timeout | undefined;
  private retry: NodeJS.Timeout | undefined;
  private failures = 0;
  private target: { root: string; options: CoreOptions } | undefined;
  private update(state: ConnectionState) {
    this.state = state;
    this.emit("state", state);
  }
  disconnect(): void {
    ++this.generation;
    this.target = undefined;
    clearInterval(this.heartbeat);
    clearTimeout(this.retry);
    this.heartbeat = undefined;
    this.retry = undefined;
    this.client?.close();
    this.client = undefined;
    this.update({ phase: "disconnected" });
  }
  async connect(
    root: string,
    options: CoreOptions,
    start = false,
  ): Promise<void> {
    this.disconnect();
    this.failures = 0;
    this.target = { root, options };
    const generation = this.generation;
    this.update({ phase: "connecting", root });
    try {
      const location = await locations(root, options.env);
      let client: CoreClient;
      let hello: HelloResult;
      try {
        ({ client, hello } = await this.attach(location, options));
      } catch (error) {
        if (
          !start ||
          !(error instanceof ConnectionError) ||
          error.code !== "unavailable"
        )
          throw error;
        if (generation !== this.generation) return;
        const pid = await launchCore(
          location,
          options,
          () => generation === this.generation,
        );
        this.emit("started", pid);
        const deadline = Date.now() + 8000;
        while (true) {
          if (generation !== this.generation) return;
          try {
            ({ client, hello } = await this.attach(location, options));
            break;
          } catch (failure) {
            if (
              !(failure instanceof ConnectionError) ||
              failure.code !== "unavailable" ||
              Date.now() >= deadline
            )
              throw failure;
            await new Promise((resolve) => setTimeout(resolve, 100));
          }
        }
      }
      if (generation !== this.generation) {
        client.close();
        return;
      }
      this.update({ phase: "connecting", root: location.root, hello });
      this.client = client;
      client.on("closed", (error) => {
        if (generation !== this.generation) return;
        this.client = undefined;
        clearInterval(this.heartbeat);
        this.update({
          ...this.state,
          phase: "error",
          message: safeMessage(error),
        });
        if (
          error instanceof ConnectionError &&
          ["unavailable", "timeout"].includes(error.code)
        )
          this.scheduleReconnect(generation);
      });
      await this.refresh();
      if (generation !== this.generation) return;
      this.heartbeat = setInterval(() => {
        void this.refresh().catch(() => {});
      }, 5000);
      this.heartbeat.unref();
    } catch (error) {
      if (generation === this.generation) {
        this.client?.close();
        this.client = undefined;
        this.update({ phase: "error", root, message: safeMessage(error) });
      }
      throw error instanceof ConnectionError
        ? error
        : new ConnectionError("configuration", safeMessage(error));
    }
  }
  private async attach(
    location: Locations,
    options: CoreOptions,
  ): Promise<{ client: CoreClient; hello: HelloResult }> {
    let token: string;
    try {
      const file = await open(location.tokenPath, "r");
      try {
        const info = await file.stat();
        if (!info.isFile() || info.size !== 64)
          throw new ConnectionError(
            "authentication",
            "Core 인증 파일 형식이 잘못됐습니다.",
          );
        const bytes = Buffer.alloc(64);
        const read = await file.read(bytes, 0, 64, 0);
        if (read.bytesRead !== 64)
          throw new ConnectionError(
            "authentication",
            "Core 인증 파일 형식이 잘못됐습니다.",
          );
        token = bytes.toString("latin1");
      } finally {
        await file.close();
      }
    } catch (error) {
      if (error instanceof ConnectionError) throw error;
      if (
        typeof error === "object" &&
        error !== null &&
        "code" in error &&
        error.code === "ENOENT"
      )
        throw new ConnectionError(
          "unavailable",
          "Core 인증 파일이 없습니다. Core 시작 상태를 확인해 주세요.",
        );
      throw new ConnectionError(
        "authentication",
        "Core 인증 파일에 접근할 수 없습니다.",
      );
    }
    if (!/^[0-9a-f]{64}$/.test(token))
      throw new ConnectionError(
        "authentication",
        "Core 인증 파일 형식이 잘못됐습니다.",
      );
    const client = await CoreClient.connect(
      location.endpoint,
      options.timeoutMs,
    );
    try {
      const params: HelloParams = {
        clientName: "telos-weaver-extension",
        clientVersion: "0.0.0",
        token,
      };
      const hello = helloResult(await client.request("core.hello", params));
      if (
        (options.projectId && hello.projectId !== options.projectId) ||
        (options.hostId && hello.hostId !== options.hostId)
      ) {
        throw new ConnectionError(
          "identity",
          "Core의 Project 또는 Host가 설정과 다릅니다.",
        );
      }
      return { client, hello };
    } catch (error) {
      client.close();
      throw error;
    }
  }
  async refresh(): Promise<void> {
    const client = this.client;
    if (!client)
      throw new ConnectionError("unavailable", "Core 연결이 없습니다.");
    try {
      const health = healthResult(await client.request("core.health", {}));
      if (client === this.client)
        this.update({
          ...this.state,
          phase: "connected",
          health,
          message: undefined,
        });
    } catch (error) {
      client.close();
      throw error;
    }
  }
  private scheduleReconnect(generation: number): void {
    const target = this.target;
    if (!target || this.retry || this.failures >= 5) return;
    const attempt = ++this.failures;
    this.retry = setTimeout(
      async () => {
        this.retry = undefined;
        if (generation !== this.generation) return;
        try {
          const location = await locations(target.root, target.options.env);
          const { client, hello } = await this.attach(location, target.options);
          if (generation !== this.generation) {
            client.close();
            return;
          }
          this.update({ phase: "connecting", root: location.root, hello });
          this.client = client;
          client.on("closed", (error) => {
            if (generation !== this.generation) return;
            this.client = undefined;
            clearInterval(this.heartbeat);
            this.update({
              ...this.state,
              phase: "error",
              message: safeMessage(error),
            });
            if (
              error instanceof ConnectionError &&
              ["unavailable", "timeout"].includes(error.code)
            )
              this.scheduleReconnect(generation);
          });
          await this.refresh();
          this.heartbeat = setInterval(() => {
            void this.refresh().catch(() => {});
          }, 5000);
          this.heartbeat.unref();
        } catch (error) {
          if (generation === this.generation) {
            this.update({
              ...this.state,
              phase: "error",
              message: safeMessage(error),
            });
            if (
              error instanceof ConnectionError &&
              ["unavailable", "timeout"].includes(error.code)
            )
              this.scheduleReconnect(generation);
          }
        }
      },
      Math.min(250 * 2 ** (attempt - 1), 4000),
    );
    this.retry.unref();
  }
}
export function safeMessage(error: unknown): string {
  return error instanceof ConnectionError
    ? error.message
    : "Core 구성 또는 파일 접근을 확인할 수 없습니다.";
}
export async function launchCore(
  location: Locations,
  options: CoreOptions,
  wanted = () => true,
): Promise<number> {
  if (!path.isAbsolute(options.executable))
    throw new ConnectionError(
      "configuration",
      "Core 실행 파일의 절대 경로를 설정해 주세요.",
    );
  let executable: string;
  try {
    executable = await realpath(options.executable);
    if (
      !(await stat(executable)).isFile() ||
      (process.platform === "win32" && !/\.(exe|com)$/i.test(executable))
    )
      throw new Error();
    const version = await promisify(execFile)(executable, ["--version"], {
      timeout: 5000,
      maxBuffer: 4096,
      windowsHide: true,
      env: options.env,
    });
    if (!/^telos-weaver \d+\.\d+\.\d+(?:[-+][\w.-]+)?\s*$/.test(version.stdout))
      throw new Error();
  } catch {
    throw new ConnectionError(
      "configuration",
      "유효한 telos-weaver 실행 파일을 확인하지 못했습니다.",
    );
  }
  if (!wanted())
    throw new ConnectionError("disconnected", "Core 시작 요청을 해제했습니다.");
  const projectId = options.projectId || `project-${location.key}`;
  const hostId =
    options.hostId ||
    (process.platform === "win32" ? "windows-local" : "unix-local");
  return new Promise((resolve, reject) => {
    // No policy, shell, installer or Cargo invocation; closing the UI must not stop Core.
    const child = spawn(
      executable,
      [
        "serve",
        "--project-root",
        location.root,
        "--project-id",
        projectId,
        "--host-id",
        hostId,
      ],
      {
        detached: true,
        windowsHide: true,
        stdio: "ignore",
        env: options.env,
      },
    );
    child.once("error", () =>
      reject(
        new ConnectionError("startup", "Core 프로세스를 시작하지 못했습니다."),
      ),
    );
    child.once("spawn", () => {
      child.unref();
      resolve(child.pid!);
    });
  });
}
