// Read-only observation and replay of a known Run (Spec §15/17).
import { CoreClient, ConnectionError, record } from "./client";
import type { ApiNotification } from "./generated/ApiNotification";
import type { ProviderEvent } from "./generated/ProviderEvent";
import { eventKinds } from "./generated/eventKinds";

export const EVENT_SCHEMA_VERSION = "0.2";

export class RunFeed {
  private id: string | undefined;
  private cursor: number;
  private disposed = false;
  private retries = 0;
  private started = false;
  constructor(
    private client: CoreClient,
    private runId: string,
    private event: (event: ProviderEvent) => void,
    private failed: (error: Error) => void,
    after = 0,
  ) {
    this.cursor = after;
  }
  get lastSequence() {
    return this.cursor;
  }
  async start(): Promise<void> {
    if (this.started || this.disposed)
      throw new ConnectionError(
        "configuration",
        "이미 시작하거나 종료한 Run 구독입니다.",
      );
    this.started = true;
    this.client.on("notification", this.receive);
    this.client.on("closed", this.closed);
    try {
      await this.subscribe();
    } catch (error) {
      this.dispose();
      throw error;
    }
  }
  private async subscribe(): Promise<void> {
    // Core acknowledges before replay; bind synchronously without speculative queues (§17, REQ-016).
    this.id = undefined;
    await this.client.request(
      "runs.subscribe",
      { runId: this.runId, afterSequence: this.cursor },
      (result) => {
        if (
          !record(result) ||
          typeof result.subscriptionId !== "string" ||
          !result.subscriptionId
        )
          throw new ConnectionError("protocol", "Run 구독 응답이 잘못됐습니다.");
        if (this.disposed) {
          void this.client
            .request("runs.unsubscribe", { subscriptionId: result.subscriptionId })
            .catch(() => {});
          return;
        }
        this.id = result.subscriptionId;
      },
    );
  }
  private receive = (notification: ApiNotification): void => {
    if (this.disposed) return;
    const params = notification.params;
    if (!this.id || !record(params) || params.subscriptionId !== this.id) return;
    try {
      if (notification.method === "runs.event") {
        const value = params.event;
        if (
          !record(value) ||
          value.runId !== this.runId ||
          value.schemaVersion !== EVENT_SCHEMA_VERSION ||
          typeof value.eventId !== "string" ||
          !value.eventId ||
          typeof value.occurredAt !== "string" ||
          !Number.isSafeInteger(value.sequence) ||
          !eventKinds.some((kind) => kind === value.kind) ||
          !record(value.payload)
        ) {
          throw new ConnectionError(
            "protocol",
            "Run 이벤트 형식 또는 정체성이 잘못됐습니다.",
          );
        }
        const sequence = value.sequence as number;
        if (sequence <= this.cursor) return;
        if (sequence !== this.cursor + 1)
          throw new ConnectionError(
            "events_unavailable",
            "Run 이벤트 순서가 끊어졌습니다. 다시 조회해 주세요.",
          );
        this.cursor = sequence;
        this.event(value as unknown as ProviderEvent);
      } else if (
        !Number.isSafeInteger(params.lastSequence) ||
        (params.lastSequence as number) < 0
      )
        throw new ConnectionError("protocol", "Run 종료 커서가 잘못됐습니다.");
      else if (
        params.reason === "run_finished" ||
        params.reason === "unsubscribed"
      )
        this.dispose();
      else if (params.reason === "slow_consumer" && ++this.retries <= 5) {
        // Only advance from events actually received, not a future server cursor.
        void this.subscribe().catch((error) => this.stop(error));
      } else
        throw new ConnectionError(
          "events_unavailable",
          "Run 구독이 종료됐습니다. 상태를 다시 조회해 주세요.",
        );
    } catch (error) {
      this.stop(
        error instanceof Error ? error : new Error("Run 이벤트 처리 실패"),
      );
    }
  };
  private stop(error: Error) {
    if (this.disposed) return;
    this.dispose();
    this.failed(error);
  }
  private closed = () =>
    this.stop(new ConnectionError("unavailable", "Run 연결이 종료됐습니다."));
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.client.removeListener("notification", this.receive);
    this.client.removeListener("closed", this.closed);
    if (this.id)
      void this.client
        .request("runs.unsubscribe", { subscriptionId: this.id })
        .catch(() => {});
  }
}
