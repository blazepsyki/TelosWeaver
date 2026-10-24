// Bounded Local Core API transport (Spec §3.3, §17; REQ-005/007).
import { EventEmitter } from "node:events";
import { createConnection, Socket } from "node:net";
import type { ApiMethod, ApiRequest } from "./generated/ApiRequest";
import type { ApiResponse } from "./generated/ApiResponse";
import type { ApiNotification } from "./generated/ApiNotification";
import type { HelloResult } from "./generated/ApiHelloResult";
import type { HealthResult } from "./generated/ApiHealthResult";

export const PROTOCOL_VERSION = "0.2";
export const MAX_FRAME_BYTES = 1024 * 1024;
export class ConnectionError extends Error {
  constructor(
    public readonly code: string,
    message: string,
  ) {
    super(message);
  }
}
export function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
export function helloResult(value: unknown): HelloResult {
  if (
    !record(value) ||
    [
      "serverVersion",
      "protocolVersion",
      "projectId",
      "hostId",
      "connectionId",
    ].some((k) => typeof value[k] !== "string" || !value[k])
  ) {
    throw new ConnectionError(
      "protocol",
      "Core 인증 응답 형식이 잘못됐습니다.",
    );
  }
  if (value.protocolVersion !== PROTOCOL_VERSION)
    throw new ConnectionError("version", "Core API 버전이 호환되지 않습니다.");
  return value as unknown as HelloResult;
}
export function healthResult(value: unknown): HealthResult {
  const status = (v: unknown) =>
    v === "ok" || v === "degraded" || v === "unavailable";
  if (
    !record(value) ||
    !status(value.status) ||
    typeof value.serverVersion !== "string" ||
    !Array.isArray(value.components) ||
    value.components.length > 128 ||
    value.components.some(
      (c) =>
        !record(c) ||
        typeof c.name !== "string" ||
        !status(c.status) ||
        (c.detail !== undefined &&
          c.detail !== null &&
          typeof c.detail !== "string"),
    )
  ) {
    throw new ConnectionError(
      "protocol",
      "Core Health 응답 형식이 잘못됐습니다.",
    );
  }
  return value as unknown as HealthResult;
}
type Pending = {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timer: NodeJS.Timeout;
};
export class CoreClient extends EventEmitter {
  private buffer = Buffer.alloc(0);
  private pending = new Map<string, Pending>();
  private serial = 0;
  private ended = false;
  private constructor(
    private socket: Socket,
    private timeoutMs: number,
  ) {
    super();
    socket.on("data", (data) => this.receive(data));
    socket.on("error", () =>
      this.finish(
        new ConnectionError("unavailable", "Core 연결이 끊어졌습니다."),
      ),
    );
    socket.on("close", () =>
      this.finish(
        this.buffer.length
          ? new ConnectionError(
              "protocol",
              "완성되지 않은 Core 프레임에서 연결이 종료됐습니다.",
            )
          : new ConnectionError("unavailable", "Core 연결이 종료됐습니다."),
      ),
    );
  }
  static connect(endpoint: string, timeoutMs = 5000): Promise<CoreClient> {
    return new Promise((resolve, reject) => {
      const socket = createConnection(endpoint);
      const timer = setTimeout(() => {
        socket.destroy();
        reject(
          new ConnectionError("timeout", "Core 연결 시간이 초과됐습니다."),
        );
      }, timeoutMs);
      const failed = (error: NodeJS.ErrnoException) => {
        clearTimeout(timer);
        socket.destroy();
        reject(
          ["EACCES", "EPERM"].includes(error.code ?? "")
            ? new ConnectionError(
                "authentication",
                "Core 연결 권한이 없습니다.",
              )
            : new ConnectionError(
                "unavailable",
                "실행 중인 Core에 연결할 수 없습니다.",
              ),
        );
      };
      socket.once("error", failed);
      socket.once("connect", () => {
        clearTimeout(timer);
        socket.removeListener("error", failed);
        resolve(new CoreClient(socket, timeoutMs));
      });
    });
  }
  request(method: ApiMethod, params: unknown): Promise<unknown> {
    if (this.ended)
      return Promise.reject(
        new ConnectionError("unavailable", "Core 연결이 없습니다."),
      );
    if (this.pending.size >= 32)
      return Promise.reject(
        new ConnectionError("limit", "대기 요청 상한에 도달했습니다."),
      );
    const requestId = `ext-${++this.serial}`;
    const request: ApiRequest = {
      protocolVersion: PROTOCOL_VERSION,
      requestId,
      method,
      params,
    };
    const frame = Buffer.from(JSON.stringify(request) + "\n");
    if (frame.length - 1 > MAX_FRAME_BYTES)
      return Promise.reject(
        new ConnectionError("limit", "요청 프레임이 너무 큽니다."),
      );
    return new Promise((resolve, reject) => {
      const timer = setTimeout(
        () =>
          this.finish(
            new ConnectionError("timeout", "Core 응답 시간이 초과됐습니다."),
          ),
        this.timeoutMs,
      );
      this.pending.set(requestId, { resolve, reject, timer });
      this.socket.write(frame, (error) => {
        if (error)
          this.finish(
            new ConnectionError(
              "unavailable",
              "Core 요청을 전송하지 못했습니다.",
            ),
          );
      });
    });
  }
  private receive(data: Buffer): void {
    try {
      let start = 0;
      while (start < data.length) {
        const end = data.indexOf(10, start);
        const piece = data.subarray(start, end < 0 ? data.length : end);
        if (this.buffer.length + piece.length > MAX_FRAME_BYTES)
          throw new ConnectionError(
            "protocol",
            "Core 프레임 크기 상한을 초과했습니다.",
          );
        this.buffer = Buffer.concat([this.buffer, piece]);
        if (end < 0) return;
        const line = new TextDecoder("utf-8", { fatal: true }).decode(
          this.buffer,
        );
        this.buffer = Buffer.alloc(0);
        this.message(JSON.parse(line));
        start = end + 1;
      }
    } catch (error) {
      this.finish(
        error instanceof ConnectionError
          ? error
          : new ConnectionError("protocol", "Core 프레임 형식이 잘못됐습니다."),
      );
    }
  }
  private message(value: unknown): void {
    if (!record(value) || value.protocolVersion !== PROTOCOL_VERSION)
      throw new ConnectionError(
        "version",
        "Core API 버전이 호환되지 않습니다.",
      );
    if (typeof value.requestId === "string") {
      if ("result" in value === "error" in value)
        throw new ConnectionError("protocol", "Core 응답 형식이 잘못됐습니다.");
      const response = value as unknown as ApiResponse;
      const pending = this.pending.get(response.requestId);
      if (!pending)
        throw new ConnectionError(
          "protocol",
          "Core 응답의 요청 ID가 일치하지 않습니다.",
        );
      if (
        "error" in response &&
        (!record(response.error) || typeof response.error.code !== "string")
      )
        throw new ConnectionError("protocol", "Core 오류 형식이 잘못됐습니다.");
      this.pending.delete(response.requestId);
      clearTimeout(pending.timer);
      if ("error" in response)
        pending.reject(
          new ConnectionError(
            response.error.code,
            response.error.code === "unauthenticated"
              ? "Core 인증에 실패했습니다."
              : "Core가 요청을 거부했습니다.",
          ),
        );
      else pending.resolve(response.result);
    } else if (
      (value.method === "runs.event" ||
        value.method === "runs.subscriptionClosed") &&
      record(value.params)
    ) {
      this.emit("notification", value as unknown as ApiNotification);
    } else
      throw new ConnectionError("protocol", "알 수 없는 Core 메시지입니다.");
  }
  close(): void {
    this.finish(
      new ConnectionError("disconnected", "Core 연결을 해제했습니다."),
    );
  }
  private finish(error: ConnectionError): void {
    if (this.ended) return;
    this.ended = true;
    this.socket.destroy();
    this.buffer = Buffer.alloc(0);
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.pending.clear();
    this.emit("closed", error);
  }
}
