const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const { CoreConnection, launchCore } = require("../dist/core");
const { CoreClient } = require("../dist/client");
const { server, reply, environment, hello, health } = require("./helpers.cjs");
const waitFor = async (predicate, timeoutMs = 4000) => {
  const until = Date.now() + timeoutMs;
  while (!predicate()) {
    if (Date.now() > until) throw new Error("state timeout");
    await new Promise((r) => setTimeout(r, 10));
  }
};

test("authentication, degraded Health and detach preserve the existing server", async (t) => {
  const env = await environment();
  t.after(env.close);
  let authenticated = 0;
  const fixture = await server((r, s) => {
    if (r.method === "core.hello") {
      assert.equal(r.params.token, env.token);
      authenticated++;
      reply(s, r, hello);
    } else reply(s, r, health);
  }, env.location.endpoint);
  t.after(fixture.close);
  const connection = new CoreConnection();
  t.after(() => connection.disconnect());
  await connection.connect(env.root, { executable: "", env: env.env });
  assert.equal(connection.state.phase, "connected");
  assert.equal(connection.state.health.status, "degraded");
  connection.disconnect();
  assert.equal(connection.state.phase, "disconnected");
  const other = await CoreClient.connect(fixture.address);
  t.after(() => other.close());
  assert.deepEqual(await other.request("core.health", {}), health);
  assert.equal(authenticated, 1);
});
test("identity failure does not try to launch another Core", async (t) => {
  const env = await environment();
  t.after(env.close);
  const fixture = await server(
    (r, s) => reply(s, r, hello),
    env.location.endpoint,
  );
  t.after(fixture.close);
  const connection = new CoreConnection();
  t.after(() => connection.disconnect());
  await assert.rejects(
    connection.connect(
      env.root,
      {
        executable: "invalid-should-not-run",
        hostId: "other-host",
        env: env.env,
      },
      true,
    ),
    (e) => e.code === "identity",
  );
  assert.equal(connection.state.phase, "error");
});
test("reconnection rereads rotated credentials and never restarts the daemon", async (t) => {
  const env = await environment();
  t.after(env.close);
  let current = env.token;
  let hellos = 0;
  const handler = (r, s) => {
    if (r.method === "core.hello") {
      assert.equal(r.params.token, current);
      hellos++;
      reply(s, r, hello);
    } else reply(s, r, health);
  };
  let fixture = await server(handler, env.location.endpoint);
  t.after(() => fixture.close());
  const connection = new CoreConnection();
  t.after(() => connection.disconnect());
  await connection.connect(env.root, { executable: "", env: env.env });
  await fixture.close();
  current = "a".repeat(64);
  await fs.writeFile(env.location.tokenPath, current);
  fixture = await server(handler, env.location.endpoint);
  await waitFor(() => hellos === 2 && connection.state.phase === "connected");
  assert.equal(connection.state.hello.hostId, "fixture-host");
});
test("a late handshake cannot revive a disconnected connection", async (t) => {
  const env = await environment();
  t.after(env.close);
  let release;
  const received = new Promise((resolve) => {
    release = resolve;
  });
  let pending;
  const fixture = await server((r, s) => {
    pending = () => reply(s, r, hello);
    release();
  }, env.location.endpoint);
  t.after(fixture.close);
  const connection = new CoreConnection();
  t.after(() => connection.disconnect());
  const connecting = connection.connect(env.root, {
    executable: "",
    env: env.env,
  });
  await received;
  connection.disconnect();
  pending();
  await connecting;
  assert.equal(connection.state.phase, "disconnected");
  assert.equal(connection.client, undefined);
});
test("invalid token and relative executable paths fail without revealing contents", async (t) => {
  const env = await environment();
  t.after(env.close);
  await fs.writeFile(env.location.tokenPath, "fixture-private-secret");
  const connection = new CoreConnection();
  t.after(() => connection.disconnect());
  await assert.rejects(
    connection.connect(env.root, { executable: "", env: env.env }, true),
    (e) =>
      e.code === "authentication" &&
      !String(e).includes("fixture-private-secret"),
  );
  await fs.writeFile(env.location.tokenPath, Buffer.alloc(64, 0xe1));
  await assert.rejects(
    connection.connect(env.root, { executable: "", env: env.env }, true),
    (e) => e.code === "authentication",
  );
  await assert.rejects(
    launchCore(env.location, { executable: "telos-weaver.exe" }),
    (e) => e.code === "configuration",
  );
});

test("six separate outages each recover and reread the rotated token", async (t) => {
  const env = await environment();
  t.after(env.close);
  let current = env.token;
  let hellos = 0;
  const handler = (r, s) => {
    if (r.method === "core.hello") {
      assert.equal(r.params.token, current);
      hellos++;
      reply(s, r, hello);
    } else reply(s, r, health);
  };
  let fixture = await server(handler, env.location.endpoint);
  t.after(() => fixture.close());
  const connection = new CoreConnection();
  t.after(() => connection.disconnect());
  let starts = 0;
  connection.on("started", () => starts++);
  await connection.connect(env.root, { executable: "", env: env.env });
  for (let cycle = 1; cycle <= 6; cycle++) {
    await fixture.close();
    current = String(cycle).repeat(64);
    await fs.writeFile(env.location.tokenPath, current);
    fixture = await server(handler, env.location.endpoint);
    await waitFor(() => hellos === cycle + 1 && connection.state.phase === "connected");
    assert.equal(connection.state.health.status, "degraded");
  }
  assert.equal(starts, 0);
});

test("hello alone does not reset the five consecutive reconnect failures", async (t) => {
  const env = await environment();
  t.after(env.close);
  let socket;
  let failHealth = false;
  let attempts = 0;
  const fixture = await server((r, s) => {
    socket = s;
    if (r.method === "core.hello") {
      if (failHealth) attempts++;
      reply(s, r, hello);
    } else if (failHealth) s.destroy();
    else reply(s, r, health);
  }, env.location.endpoint);
  t.after(fixture.close);
  const connection = new CoreConnection();
  t.after(() => connection.disconnect());
  await connection.connect(env.root, { executable: "", env: env.env });
  failHealth = true;
  socket.destroy();
  await waitFor(() => attempts === 5 && connection.state.phase === "error", 12000);
  await new Promise((resolve) => setTimeout(resolve, 500));
  assert.equal(attempts, 5);
  failHealth = false;
  await connection.connect(env.root, { executable: "", env: env.env });
  assert.equal(connection.state.phase, "connected");
});

test("disconnect during reconnect Health prevents a late success from restoring state", async (t) => {
  const env = await environment();
  t.after(env.close);
  let socket;
  let hold = false;
  let release;
  const fixture = await server((r, s) => {
    socket = s;
    if (r.method === "core.hello") reply(s, r, hello);
    else if (hold) release = () => reply(s, r, health);
    else reply(s, r, health);
  }, env.location.endpoint);
  t.after(fixture.close);
  const connection = new CoreConnection();
  t.after(() => connection.disconnect());
  await connection.connect(env.root, { executable: "", env: env.env });
  hold = true;
  socket.destroy();
  await waitFor(() => release);
  connection.disconnect();
  release();
  await new Promise((resolve) => setTimeout(resolve, 300));
  assert.equal(connection.state.phase, "disconnected");
  assert.equal(connection.client, undefined);
});

// Spec §2.1/§17, G3 finding 3: stale values must not survive either failure path.
test("socket loss and a failed reconnect clear Health and identity before fresh recovery", async (t) => {
  const env = await environment();
  t.after(env.close);
  let identity = hello;
  let currentHealth = health;
  const handler = (r, s) => reply(s, r, r.method === "core.hello" ? identity : currentHealth);
  let fixture = await server(handler, env.location.endpoint);
  t.after(() => fixture.close());
  const connection = new CoreConnection();
  t.after(() => connection.disconnect());
  const errors = [];
  connection.on("state", (state) => {
    if (state.phase === "error") errors.push(state);
  });
  await connection.connect(env.root, { executable: "", env: env.env });
  await fixture.close();
  await waitFor(() => errors.length >= 2);
  for (const state of errors) {
    assert.equal(state.health, undefined);
    assert.equal(state.hello, undefined);
    assert.equal(state.root, env.location.root);
    assert.ok(state.message);
  }
  identity = { ...hello, serverVersion: "0.0.1", hostId: "replacement-host" };
  currentHealth = { ...health, status: "ok", components: [{ name: "fresh", status: "ok" }] };
  fixture = await server(handler, env.location.endpoint);
  await waitFor(() => connection.state.phase === "connected");
  assert.deepEqual(connection.state.hello, identity);
  assert.deepEqual(connection.state.health, currentHealth);
});

test("Heartbeat failure and reconnect Health failure discard old values and recover", async (t) => {
  const env = await environment();
  t.after(env.close);
  let failHealth = false;
  let currentHealth = health;
  const fixture = await server((r, s) => {
    if (r.method === "core.hello") reply(s, r, hello);
    else if (failHealth) s.destroy();
    else reply(s, r, currentHealth);
  }, env.location.endpoint);
  t.after(fixture.close);
  const connection = new CoreConnection();
  t.after(() => connection.disconnect());
  const errors = [];
  connection.on("state", (state) => {
    if (state.phase === "error") errors.push(state);
  });
  await connection.connect(env.root, { executable: "", env: env.env });
  failHealth = true;
  // Heartbeat invokes this same refresh path without changing reconnect policy.
  await assert.rejects(connection.refresh());
  await waitFor(() => errors.length >= 2);
  for (const state of errors) {
    assert.equal(state.health, undefined);
    assert.equal(state.hello, undefined);
  }
  currentHealth = { ...health, status: "ok", components: [{ name: "replacement", status: "ok" }] };
  failHealth = false;
  await waitFor(() => connection.state.phase === "connected");
  assert.deepEqual(connection.state.health, currentHealth);
  assert.deepEqual(connection.state.hello, hello);
});
