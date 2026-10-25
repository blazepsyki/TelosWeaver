const { test } = require("node:test");
const assert = require("node:assert/strict");
const Module = require("node:module");
const { CoreConnection } = require("../dist/core");
const { hello, health } = require("./helpers.cjs");

function ui() {
  const commands = new Map();
  const messages = [];
  const output = [];
  let provider;
  const disposable = () => ({ dispose() {} });
  const folder = (name) => ({
    name,
    uri: { scheme: "file", fsPath: `C:/${name}` },
  });
  const vscode = {
    workspace: {
      isTrusted: false,
      workspaceFolders: [folder("one"), folder("two")],
      getConfiguration: () => ({
        get: (key, fallback) => (key === "corePath" ? "C:/core.exe" : fallback),
        update: async () => {},
      }),
    },
    window: {
      createOutputChannel: () => ({
        ...disposable(),
        appendLine: (line) => output.push(line),
        clear() {},
        show() {},
      }),
      createStatusBarItem: () => ({ ...disposable(), show() {} }),
      createTreeView: (_, options) => {
        provider = options.treeDataProvider;
        return disposable();
      },
      showWarningMessage: (message) => messages.push(message),
      showErrorMessage: (message) => messages.push(message),
      showQuickPick: async (values) => values[1],
    },
    commands: {
      registerCommand: (name, handler) => {
        commands.set(name, handler);
        return disposable();
      },
    },
    TreeItem: class {
      constructor(label) {
        this.label = label;
      }
    },
    EventEmitter: class {
      event = () => {};
      fire() {}
      dispose() {}
    },
    StatusBarAlignment: { Left: 1 },
    env: {},
    ConfigurationTarget: { Global: 1 },
  };
  const originalLoad = Module._load;
  Module._load = function (name, ...args) {
    return name === "vscode" ? vscode : originalLoad.call(this, name, ...args);
  };
  let extension;
  try {
    delete require.cache[require.resolve("../dist/extension")];
    extension = require("../dist/extension");
  } finally {
    Module._load = originalLoad;
  }
  const context = { subscriptions: [] };
  const api = extension.activate(context);
  return {
    vscode,
    commands,
    messages,
    output,
    api,
    provider: () => provider,
    close: () => context.subscriptions.forEach((item) => item.dispose()),
  };
}
test("Restricted Mode and empty workspaces block programmatic start/connect", async () => {
  const fixture = ui();
  const original = CoreConnection.prototype.connect;
  let calls = 0;
  CoreConnection.prototype.connect = async () => {
    calls++;
  };
  try {
    await fixture.commands.get("telosWeaver.startCore")();
    await fixture.commands.get("telosWeaver.connect")();
    assert.equal(calls, 0);
    assert.equal(fixture.messages.length, 2);
    fixture.vscode.workspace.isTrusted = true;
    fixture.vscode.workspace.workspaceFolders = [];
    await fixture.commands.get("telosWeaver.startCore")();
    assert.equal(calls, 0);
  } finally {
    fixture.close();
    CoreConnection.prototype.connect = original;
  }
});
test("multi-root selection displays real identity and Degraded without auto-start", async () => {
  const fixture = ui();
  const original = CoreConnection.prototype.connect;
  const calls = [];
  CoreConnection.prototype.connect = async function (root, options, start) {
    calls.push({ root, options, start });
    this.state = { phase: "connected", root, hello, health };
    this.emit("state", this.state);
  };
  try {
    assert.equal(calls.length, 0);
    fixture.vscode.workspace.isTrusted = true;
    await fixture.commands.get("telosWeaver.connect")();
    assert.equal(calls[0].root, "C:/two");
    assert.equal(calls[0].start, false);
    const rows = fixture.provider().getChildren();
    assert.ok(
      rows.some(
        (row) => row.label === "Host" && row.description === "fixture-host",
      ),
    );
    assert.ok(rows.some((row) => row.description.includes("degraded")));
    assert.deepEqual(
      fixture.output,
      ["미연결", "연결됨 (degraded)"].filter((x) => x !== "미연결"),
    );
  } finally {
    fixture.close();
    CoreConnection.prototype.connect = original;
  }
});

test("repeated Health refreshes update the view and log only status transitions", async () => {
  const fixture = ui();
  const original = CoreConnection.prototype.connect;
  let connection;
  CoreConnection.prototype.connect = async function (root) {
    connection = this;
    this.state = { phase: "connected", root, hello, health };
    this.emit("state", this.state);
  };
  const publish = (state) => {
    connection.state = state;
    connection.emit("state", state);
  };
  try {
    fixture.vscode.workspace.isTrusted = true;
    await fixture.commands.get("telosWeaver.connect")();
    for (let count = 0; count < 4; count++) publish({ ...connection.state });
    publish({ ...connection.state, health: {
      ...health, components: [{ name: "execution", status: "ok" }],
    } });
    assert.deepEqual(fixture.output, ["연결됨 (degraded)"]);
    assert.ok(fixture.provider().getChildren().some(
      (row) => row.label === "execution" && row.description === "ok",
    ));
    publish({ ...connection.state, health: { ...health, status: "ok" } });
    publish({ ...connection.state, phase: "error", message: "fixture" });
    publish({ ...connection.state, phase: "connected", message: undefined });
    assert.deepEqual(fixture.output, ["연결됨 (degraded)", "연결됨", "연결 오류", "연결됨"]);
  } finally {
    fixture.close();
    CoreConnection.prototype.connect = original;
  }
});

test("RN Health changes refresh its row while the Core connection and output stay stable", async () => {
  const fixture = ui();
  const original = CoreConnection.prototype.connect;
  let connection;
  let calls = 0;
  CoreConnection.prototype.connect = async function (root) {
    calls++;
    connection = this;
    this.state = { phase: "connected", root, hello, health };
    this.emit("state", this.state);
  };
  try {
    fixture.vscode.workspace.isTrusted = true;
    await fixture.commands.get("telosWeaver.connect")();
    for (const status of ["ok", "degraded", "ok"]) {
      connection.state = {
        ...connection.state,
        health: {
          ...health,
          components: [
            { name: "execution", status: "ok" },
            { name: "rn", status, detail: "fixture RN" },
          ],
        },
      };
      connection.emit("state", connection.state);
      const rows = fixture.provider().getChildren();
      assert.ok(rows.some((row) => row.label === "rn" && row.description === status));
      assert.ok(rows.some((row) => row.label === "execution" && row.description === "ok"));
      assert.equal(fixture.api.state().phase, "connected");
    }
    assert.equal(calls, 1);
    assert.deepEqual(fixture.output, ["연결됨 (degraded)"]);
  } finally {
    fixture.close();
    CoreConnection.prototype.connect = original;
  }
});
