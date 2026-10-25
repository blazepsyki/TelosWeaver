// Explicit local Core test with no policy; no model or provider invocation.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const { CoreConnection } = require("../dist/core");
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
  },
);
