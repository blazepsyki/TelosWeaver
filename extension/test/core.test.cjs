const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const { CoreConnection, launchCore } = require("../dist/core");
const { CoreClient } = require("../dist/client");
const { server, reply, environment, hello, health } = require("./helpers.cjs");
const waitFor = async (predicate) => {
  const until = Date.now() + 4000;
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
