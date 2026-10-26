// Actual VS Code API and actual Core; no provider calls (Spec §19.5.8).
import * as vscode from "vscode";
import * as assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { CoreConnection, ConnectionState } from "../core";

export async function run(): Promise<void> {
  const results: string[] = [];
  const pids: number[] = [];
  const original = CoreConnection.prototype.emit;
  CoreConnection.prototype.emit = function (
    name: string | symbol,
    ...args: unknown[]
  ) {
    if (name === "started" && typeof args[0] === "number") pids.push(args[0]);
    return original.call(this, name, ...args);
  };
  let api: { state: () => ConnectionState; disconnect: () => void } | undefined;
  let completed = false;
  try {
    const extension = vscode.extensions.all.find(
      (item) => item.packageJSON.name === "telos-weaver",
    );
    assert.ok(extension);
    api = await extension.activate();
    assert.ok(api);
    assert.equal(api.state().phase, "disconnected");
    results.push("activation does not start Core");
    const commands = await vscode.commands.getCommands();
    for (const command of [
      "startCore",
      "connect",
      "disconnect",
      "refresh",
      "watchRun",
      "showOutput",
    ])
      assert.ok(commands.includes(`telosWeaver.${command}`));
    results.push("native view and lifecycle commands registered");
    await vscode.workspace
      .getConfiguration("telosWeaver")
      .update(
        "corePath",
        process.env.TELOS_CORE_EXECUTABLE,
        vscode.ConfigurationTarget.Global,
      );
    await vscode.commands.executeCommand("telosWeaver.startCore");
    assert.equal(api.state().phase, "connected");
    assert.equal(api.state().health?.status, "degraded");
    assert.equal(pids.length, 1);
    results.push(
      "Start command authenticates to real policy-free Core and displays Degraded",
    );
    const pid = pids[0]!;
    await vscode.commands.executeCommand("telosWeaver.disconnect");
    assert.equal(api.state().phase, "disconnected");
    assert.equal(api.state().health, undefined);
    assert.equal(api.state().hello, undefined);
    results.push("Disconnected public state has no current Health or Core identity");
    process.kill(pid, 0);
    await vscode.commands.executeCommand("telosWeaver.connect");
    assert.equal(api.state().phase, "connected");
    assert.ok(api.state().hello);
    assert.ok(api.state().health);
    assert.equal(pids.length, 1);
    results.push("Disconnect keeps Core alive; Connect reuses its PID");
    await vscode.commands.executeCommand("telosWeaver.refresh");
    await vscode.commands.executeCommand("telosWeaver.showOutput");
    // Dispose the extension's registered resources, as Extension Host unload does.
    api.disconnect();
    process.kill(pid, 0);
    results.push(
      "UI detach retains Core; no policy or provider execution enabled",
    );
    await writeFile(
      process.env.TELOS_HOST_RESULTS!,
      JSON.stringify({ passed: true, checks: results, corePid: pid }, null, 2),
    );
    completed = true;
  } catch (error) {
    await writeFile(
      process.env.TELOS_HOST_RESULTS!,
      JSON.stringify(
        {
          passed: false,
          checks: results,
          failure: error instanceof Error ? error.message : "unknown",
        },
        null,
        2,
      ),
    );
    throw error;
  } finally {
    api?.disconnect();
    CoreConnection.prototype.emit = original;
    // The outer runner confirms Core survives this entire Host before cleaning it up.
    if (!completed)
      for (const pid of pids) {
        try {
          process.kill(pid);
        } catch {}
      }
  }
}
