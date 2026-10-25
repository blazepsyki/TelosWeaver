// Minimal native UI over Local Core API (Spec §17, PH0-EXT).
import * as vscode from "vscode";
import {
  CoreConnection,
  CoreOptions,
  ConnectionState,
  safeMessage,
} from "./core";
import { RunFeed } from "./runFeed";
import { record } from "./client";
import type { CoreClient } from "./client";

class StatusProvider implements vscode.TreeDataProvider<vscode.TreeItem> {
  private changed = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this.changed.event;
  constructor(private connection: CoreConnection) {}
  refresh() {
    this.changed.fire();
  }
  getTreeItem(item: vscode.TreeItem) {
    return item;
  }
  getChildren(): vscode.TreeItem[] {
    const state = this.connection.state;
    const item = (label: string, value?: string) => {
      const row = new vscode.TreeItem(label);
      row.description = value;
      return row;
    };
    return [
      item("연결", phaseName(state)),
      ...(state.root ? [item("프로젝트 경로", state.root)] : []),
      ...(state.hello
        ? [
            item("Core", state.hello.serverVersion),
            item("Project", state.hello.projectId),
            item("Host", state.hello.hostId),
          ]
        : []),
      ...(state.health?.components.map((c) => item(c.name, c.status)) ?? []),
      ...(state.message ? [item("진단", state.message)] : []),
    ];
  }
  dispose() {
    this.changed.dispose();
  }
}
function phaseName(state: ConnectionState): string {
  if (state.phase === "connected")
    return state.health?.status === "ok"
      ? "연결됨"
      : `연결됨 (${state.health?.status ?? "확인 중"})`;
  return { disconnected: "미연결", connecting: "연결 중", error: "연결 오류" }[
    state.phase
  ];
}
export function activate(context: vscode.ExtensionContext) {
  const connection = new CoreConnection();
  const output = vscode.window.createOutputChannel("TelosWeaver");
  const status = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    20,
  );
  const tree = new StatusProvider(connection);
  const view = vscode.window.createTreeView("telosWeaver.status", {
    treeDataProvider: tree,
  });
  const feeds = new Set<RunFeed>();
  let observed: { root: string; runId: string; cursor: number } | undefined;
  let observedClient: CoreClient | undefined;
  let outputBytes = 0;
  const log = (value: string) => {
    const line = value.slice(0, 4096);
    outputBytes += Buffer.byteLength(line, "utf8");
    if (outputBytes > 256 * 1024) {
      output.clear();
      outputBytes = Buffer.byteLength(line);
    }
    output.appendLine(line);
  };
  const update = () => {
    status.text = `$(plug) TelosWeaver: ${phaseName(connection.state)}`;
    status.tooltip =
      connection.state.message || connection.state.hello?.hostId || "Core 연결";
    status.command = "telosWeaver.connect";
    status.show();
    tree.refresh();
  };
  const disposeFeeds = () => {
    for (const feed of feeds) feed.dispose();
    feeds.clear();
    observedClient = undefined;
  };
  const resumeObserved = async () => {
    const client = connection.client;
    const target = observed;
    if (
      !client ||
      !target ||
      target.root !== connection.state.root ||
      observedClient === client
    )
      return;
    observedClient = client;
    const feed = new RunFeed(
      client,
      target.runId,
      (event) => {
        target.cursor = event.sequence;
        const text =
          record(event.payload) && typeof event.payload.text === "string"
            ? event.payload.text
            : "";
        log(`${event.sequence} ${event.kind}${text ? `: ${text}` : ""}`);
      },
      (error) => log(safeMessage(error)),
      target.cursor,
    );
    feeds.add(feed);
    try {
      await feed.start();
    } catch (error) {
      feed.dispose();
      log(safeMessage(error));
    }
  };
  let lastStatus: string | undefined;
  connection.on("state", (state) => {
    if (state.phase !== "connected") disposeFeeds();
    else void resumeObserved();
    update();
    // Refresh the view on every Health response, but log transitions only (§17, REQ-016).
    const label = phaseName(state);
    if (label !== lastStatus) log(label);
    lastStatus = label;
  });
  async function selectedFolder(): Promise<vscode.WorkspaceFolder | undefined> {
    if (!vscode.workspace.isTrusted) {
      void vscode.window.showWarningMessage(
        "Core에 연결하려면 Workspace를 신뢰해야 합니다.",
      );
      return;
    }
    const folders = vscode.workspace.workspaceFolders;
    if (!folders?.length) {
      void vscode.window.showWarningMessage(
        "프로젝트 폴더를 먼저 열어 주세요.",
      );
      return;
    }
    const folder =
      folders.length === 1
        ? folders[0]
        : (
            await vscode.window.showQuickPick(
              folders.map((folder) => ({
                label: folder.name,
                description: folder.uri.fsPath,
                folder,
              })),
            )
          )?.folder;
    if (folder && !["file", "vscode-remote"].includes(folder.uri.scheme)) {
      void vscode.window.showWarningMessage(
        "실제 파일시스템의 Workspace만 지원합니다.",
      );
      return;
    }
    return folder;
  }
  async function open(start: boolean): Promise<void> {
    const folder = await selectedFolder();
    if (!folder) return;
    const config = vscode.workspace.getConfiguration("telosWeaver", folder.uri);
    const options: CoreOptions = {
      executable: config.get<string>("corePath", ""),
      projectId: config.get<string>("projectId") || undefined,
      hostId: config.get<string>("hostId") || undefined,
    };
    if (start && vscode.env.remoteName && !options.hostId) {
      void vscode.window.showWarningMessage(
        "원격 Core를 시작하려면 실행 Host의 hostId를 설정해 주세요.",
      );
      return;
    }
    try {
      await connection.connect(folder.uri.fsPath, options, start);
    } catch (error) {
      const message = safeMessage(error);
      log(message);
      void vscode.window.showErrorMessage(message);
    }
  }
  const command = (name: string, handler: (...args: unknown[]) => unknown) =>
    context.subscriptions.push(vscode.commands.registerCommand(name, handler));
  command("telosWeaver.connect", () => open(false));
  command("telosWeaver.startCore", () => open(true));
  command("telosWeaver.disconnect", () => connection.disconnect());
  command("telosWeaver.refresh", async () => {
    try {
      await connection.refresh();
    } catch (error) {
      void vscode.window.showErrorMessage(safeMessage(error));
    }
  });
  command("telosWeaver.showOutput", () => output.show());
  command("telosWeaver.selectCore", async () => {
    const files = await vscode.window.showOpenDialog({
      canSelectMany: false,
      canSelectFiles: true,
      canSelectFolders: false,
      openLabel: "Core 실행 파일 선택",
    });
    if (files?.[0])
      await vscode.workspace
        .getConfiguration("telosWeaver")
        .update("corePath", files[0].fsPath, vscode.ConfigurationTarget.Global);
  });
  command("telosWeaver.watchRun", async () => {
    if (!vscode.workspace.isTrusted || !connection.client) return;
    const runId = await vscode.window.showInputBox({
      prompt: "조회할 Run ID",
      validateInput: (value) =>
        value.trim().length > 0 && value.length <= 256
          ? undefined
          : "Run ID를 입력해 주세요.",
    });
    if (!runId || !connection.client) return;
    disposeFeeds();
    observed = { root: connection.state.root!, runId: runId.trim(), cursor: 0 };
    await resumeObserved();
    output.show();
  });
  context.subscriptions.push(
    {
      dispose: () => {
        connection.removeAllListeners();
        disposeFeeds();
        connection.disconnect();
      },
    },
    output,
    status,
    tree,
    view,
  );
  update();
  return {
    state: () => connection.state,
    disconnect: () => connection.disconnect(),
  };
}
export function deactivate(): void {}
