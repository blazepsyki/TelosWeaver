// Explicit local Core test with no policy; no model or provider invocation.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const path = require("node:path");
const { CoreConnection } = require("../dist/core");
const { CoreClient } = require("../dist/client");
const { environment } = require("./helpers.cjs");

test(
  "real Core startup, DenyRuns Health, detach and reattach",
  { skip: !process.env.TELOS_CORE_EXECUTABLE },
  async (t) => {
    const env = await environment();
    const pids = [];
    const first = new CoreConnection();
    const second = new CoreConnection();
    first.on("started", (pid) => pids.push(pid));
    second.on("started", (pid) => pids.push(pid));
    t.after(async () => {
      first.disconnect();
      second.disconnect();
      for (const pid of pids) {
        try {
          process.kill(pid);
        } catch {}
      }
      await new Promise((r) => setTimeout(r, 200));
      await env.close();
    });
    const options = {
      executable: process.env.TELOS_CORE_EXECUTABLE,
      env: env.env,
    };
    await first.connect(env.root, options, true);
    assert.equal(first.state.phase, "connected");
    assert.equal(first.state.health.status, "degraded");
    const rn = first.state.health.components.find((item) => item.name === "rn");
    assert.equal(rn?.status, "degraded");
    assert.match(rn.detail, /not configured/);
    assert.equal(first.state.hello.projectId, `project-${env.location.key}`);
    // Check actual Core credentials and Unix IPC, not fixture permissions (§3.3, SAFE-10).
    const token = await fs.readFile(env.location.tokenPath, "utf8");
    assert.match(token, /^[0-9a-f]{64}$/);
    assert.notEqual(token, env.token, "Core must replace the fixture token");
    if (process.platform !== "win32") {
      const socket = await fs.stat(env.location.endpoint);
      assert.ok(socket.isSocket());
      assert.equal(socket.mode & 0o777, 0o600);
      const credentials = await fs.stat(env.location.tokenPath);
      assert.equal(credentials.mode & 0o777, 0o600);
      for (const directory of [
        path.dirname(env.location.endpoint),
        path.dirname(env.location.tokenPath),
      ]) {
        const info = await fs.stat(directory);
        assert.equal(info.mode & 0o777, 0o700);
        assert.equal(info.uid, process.getuid());
      }
      assert.equal(socket.uid, process.getuid());
      assert.equal(credentials.uid, process.getuid());
      console.log("Unix Socket/token 0600; directories 0700; current UID owns IPC");
    }
    const unauthenticated = await CoreClient.connect(env.location.endpoint);
    try {
      await assert.rejects(
        unauthenticated.request("core.hello", {
          clientName: "telos-weaver-extension-negative-test",
          clientVersion: "0.0.0",
          token: `${token[0] === "0" ? "1" : "0"}${token.slice(1)}`,
        }),
        (e) => e.code === "unauthenticated",
      );
    } finally {
      unauthenticated.close();
    }
    console.log(`Temporary project: ${env.root}; invalid token rejected`);
    const task = {
      id: "deny-fixture",
      revision: 1,
      projectId: first.state.hello.projectId,
      planId: "plan-fixture",
      type: "general",
      objective: "Do not execute",
      constraints: [],
      dependencies: [],
      agent: { provider: "codex", role: "worker" },
      workspace: {
        mode: "none",
        hostId: first.state.hello.hostId,
        readOnly: true,
      },
      inputs: [],
      outputs: [],
      validation: [],
      permissionProfileId: "read-only",
      budgetProfileId: "default",
      completionPolicyId: "default",
    };
    await assert.rejects(
      first.client.request("runs.start", {
        task,
        session: { mode: "fresh" },
        idempotencyKey: "deny-fixture",
      }),
      (e) => e.code === "unavailable",
    );
    assert.equal(pids.length, 1);
    const pid = pids[0];
    first.disconnect();
    process.kill(pid, 0);
    await second.connect(env.root, options, true);
    assert.equal(pids.length, 1, "existing Core must be reused");
    assert.equal(second.state.phase, "connected");
    console.log(
      `Core PID ${pid}: authenticated, degraded, Run denied, retained after detach, reused`,
    );
    // G3 finding 3: actual IPC loss must invalidate current Health and identity.
    process.kill(pid);
    pids.splice(pids.indexOf(pid), 1);
    const deadline = Date.now() + 5000;
    while (second.state.phase !== "error" && Date.now() < deadline)
      await new Promise((resolve) => setTimeout(resolve, 10));
    assert.equal(second.state.phase, "error");
    assert.equal(second.state.health, undefined);
    assert.equal(second.state.hello, undefined);
    assert.equal(second.state.root, env.location.root);
    console.log(`Core PID ${pid}: connection loss removed stale Health and identity`);
  },
);
