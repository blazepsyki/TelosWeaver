const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { once } = require("node:events");
const {
  CoreClient,
  PROTOCOL_VERSION,
  MAX_FRAME_BYTES,
  helloResult,
  healthResult,
} = require("../dist/client");
const { normalizeRoot, projectKey } = require("../dist/locations");
const { server, reply, notify, hello, health } = require("./helpers.cjs");

test("project identity agrees with shared Windows/UNC/Unicode/Unix vectors", () => {
  for (const vector of JSON.parse(
    fs.readFileSync(
      path.join(__dirname, "../../schemas/vectors/project-key.json"),
      "utf8",
    ),
  )) {
    assert.equal(
      normalizeRoot(vector.canonical, vector.platform === "windows"),
      vector.normalized,
    );
    assert.equal(projectKey(vector.normalized), vector.projectKey);
  }
});
test("protocol constants agree with the Rust contract", () => {
  const source = fs.readFileSync(
    path.join(__dirname, "../../crates/schema/src/api.rs"),
    "utf8",
  );
  assert.ok(
    source.includes(`API_PROTOCOL_VERSION: &str = "${PROTOCOL_VERSION}"`),
  );
  assert.equal(MAX_FRAME_BYTES, 1024 * 1024);
  assert.ok(source.includes("MAX_FRAME_BYTES: usize = 1024 * 1024"));
  const eventSource = fs.readFileSync(
    path.join(__dirname, "../../crates/schema/src/events.rs"),
    "utf8",
  );
  assert.ok(
    eventSource.includes(
      `PROVIDER_EVENT_SCHEMA_VERSION: &str = "${require("../dist/runFeed").EVENT_SCHEMA_VERSION}"`,
    ),
  );
  assert.equal(helloResult(hello).projectId, "fixture-project");
  assert.equal(healthResult(health).status, "degraded");
  assert.throws(() => helloResult({ ...hello, hostId: "" }));
  assert.throws(() => healthResult({ status: "ok", components: [] }));
});
test("split UTF-8 frames and mixed notifications preserve request correlation", async (t) => {
  const requests = [];
  const fixture = await server((request, socket) => {
    requests.push(request);
    if (requests.length === 2) {
      notify(socket, "runs.subscriptionClosed", {
        subscriptionId: "unrelated",
        reason: "run_finished",
        lastSequence: 0,
      });
      const frame = Buffer.from(
        JSON.stringify({
          protocolVersion: "0.2",
          requestId: requests[1].requestId,
          result: "한글",
        }) + "\n",
      );
      const at = frame.indexOf(Buffer.from("한글")) + 1;
      socket.write(frame.subarray(0, at));
      socket.write(frame.subarray(at));
      reply(socket, requests[0], "first");
    }
  });
  t.after(fixture.close);
  const client = await CoreClient.connect(fixture.address);
  t.after(() => client.close());
  let notifications = 0;
  client.on("notification", () => notifications++);
  assert.deepEqual(
    await Promise.all([
      client.request("core.health", {}),
      client.request("core.health", {}),
    ]),
    ["first", "한글"],
  );
  assert.equal(notifications, 1);
});
test("malformed, incompatible and oversized frames fail closed", async (t) => {
  for (const data of [
    Buffer.from("{bad\n"),
    Buffer.from([255, 10]),
    Buffer.from(
      JSON.stringify({
        protocolVersion: "99",
        requestId: "ext-1",
        result: {},
      }) + "\n",
    ),
    Buffer.alloc(MAX_FRAME_BYTES + 1, 65),
  ]) {
    const fixture = await server((_, socket) => socket.write(data));
    const client = await CoreClient.connect(fixture.address, 1000);
    try {
      await assert.rejects(client.request("core.health", {}), (e) =>
        ["protocol", "version"].includes(e.code),
      );
    } finally {
      client.close();
      await fixture.close();
    }
  }
});
test("timeout rejects concurrent requests without provider diagnostics or tokens", async (t) => {
  const fixture = await server(() => {});
  t.after(fixture.close);
  const client = await CoreClient.connect(fixture.address, 30);
  t.after(() => client.close());
  const results = await Promise.allSettled([
    client.request("core.health", {}),
    client.request("core.hello", { token: "private-token" }),
  ]);
  assert.ok(
    results.every(
      (r) =>
        r.status === "rejected" &&
        r.reason.code === "timeout" &&
        !r.reason.message.includes("private-token"),
    ),
  );
});
test("remote errors redact their message and details", async (t) => {
  const fixture = await server((request, socket) =>
    socket.write(
      JSON.stringify({
        protocolVersion: "0.2",
        requestId: request.requestId,
        error: {
          code: "unauthenticated",
          message: "fixture-secret",
          details: "fixture-secret",
        },
      }) + "\n",
    ),
  );
  t.after(fixture.close);
  const client = await CoreClient.connect(fixture.address);
  t.after(() => client.close());
  await assert.rejects(
    client.request("core.hello", {}),
    (e) =>
      e.code === "unauthenticated" && !String(e).includes("fixture-secret"),
  );
});
test("request bounds and unknown response IDs are explicit failures", async (t) => {
  const fixture = await server((request, socket) =>
    reply(socket, { requestId: "wrong" }, {}),
  );
  t.after(fixture.close);
  const client = await CoreClient.connect(fixture.address);
  t.after(() => client.close());
  await assert.rejects(
    client.request("core.health", { text: "x".repeat(MAX_FRAME_BYTES) }),
    (e) => e.code === "limit",
  );
  await assert.rejects(
    client.request("core.health", {}),
    (e) => e.code === "protocol",
  );
});
test("disconnect during a partial frame rejects instead of accepting a truncated reply", async (t) => {
  const fixture = await server((_, socket) =>
    socket.end('{"protocolVersion":'),
  );
  t.after(fixture.close);
  const client = await CoreClient.connect(fixture.address);
  t.after(() => client.close());
  await assert.rejects(
    client.request("core.health", {}),
    (e) => e.code === "protocol",
  );
});

test("pending requests remain bounded while subscription responses are delayed", async (t) => {
  const fixture = await server(() => {});
  t.after(fixture.close);
  const client = await CoreClient.connect(fixture.address);
  t.after(() => client.close());
  const pending = Promise.allSettled(Array.from({ length: 32 }, () =>
    client.request("runs.subscribe", { runId: "fixture", afterSequence: 0 })));
  await assert.rejects(client.request("runs.subscribe", { runId: "overflow", afterSequence: 0 }),
    (error) => error.code === "limit");
  client.close();
  assert.ok((await pending).every((result) => result.status === "rejected"));
});
