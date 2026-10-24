const { test } = require("node:test");
const assert = require("node:assert/strict");
const { CoreClient } = require("../dist/client");
const { RunFeed } = require("../dist/runFeed");
const { server, reply, notify, health } = require("./helpers.cjs");
const event = (sequence) => ({
  runId: "run-fixture",
  sequence,
  kind: "text_delta",
  payload: { text: "fixture" },
  eventId: `e${sequence}`,
  schemaVersion: "0.2",
  occurredAt: "2026-10-25T00:00:00Z",
});

test("slow consumers replay from received sequence and filter foreign subscriptions", async (t) => {
  const cursors = [];
  let done;
  const ended = new Promise((resolve) => {
    done = resolve;
  });
  const fixture = await server((r, s) => {
    if (r.method !== "runs.subscribe") {
      reply(s, r, {});
      return;
    }
    cursors.push(r.params.afterSequence);
    const id = `subscription-${cursors.length}`;
    reply(s, r, { subscriptionId: id });
    notify(s, "runs.event", { subscriptionId: "foreign", event: event(50) });
    if (cursors.length === 1) {
      notify(s, "runs.event", { subscriptionId: id, event: event(1) });
      notify(s, "runs.subscriptionClosed", {
        subscriptionId: id,
        reason: "slow_consumer",
        lastSequence: 9,
      });
    } else {
      notify(s, "runs.event", { subscriptionId: id, event: event(1) });
      notify(s, "runs.event", { subscriptionId: id, event: event(2) });
      notify(s, "runs.subscriptionClosed", {
        subscriptionId: id,
        reason: "run_finished",
        lastSequence: 2,
      });
    }
  });
  t.after(fixture.close);
  const client = await CoreClient.connect(fixture.address);
  t.after(() => client.close());
  const events = [];
  const feed = new RunFeed(
    client,
    "run-fixture",
    (e) => {
      events.push(e.sequence);
      if (e.sequence === 2) done();
    },
    (e) => {
      throw e;
    },
  );
  t.after(() => feed.dispose());
  await feed.start();
  await ended;
  assert.deepEqual(events, [1, 2]);
  assert.deepEqual(cursors, [0, 1]);
  assert.equal(feed.lastSequence, 2);
});
test("sequence gaps and cursor loss stop instead of hiding missing events", async (t) => {
  for (const mode of ["gap", "lost"]) {
    let failed;
    const failure = new Promise((resolve) => {
      failed = resolve;
    });
    const fixture = await server((r, s) => {
      if (r.method !== "runs.subscribe") {
        reply(s, r, {});
        return;
      }
      reply(s, r, { subscriptionId: "sub" });
      if (mode === "gap")
        notify(s, "runs.event", { subscriptionId: "sub", event: event(2) });
      else
        notify(s, "runs.subscriptionClosed", {
          subscriptionId: "sub",
          reason: "events_unavailable",
          lastSequence: 10,
        });
    });
    const client = await CoreClient.connect(fixture.address);
    const feed = new RunFeed(
      client,
      "run-fixture",
      () => assert.fail("unexpected event"),
      failed,
    );
    try {
      await feed.start();
      assert.equal((await failure).code, "events_unavailable");
      assert.equal(client.listenerCount("notification"), 0);
    } finally {
      feed.dispose();
      client.close();
      await fixture.close();
    }
  }
});
test("unknown event schemas and kinds are not displayed as ordinary output", async () => {
  for (const invalid of [
    { ...event(1), schemaVersion: "99" },
    { ...event(1), kind: "unknown_kind" },
  ]) {
    let fail;
    const failure = new Promise((resolve) => {
      fail = resolve;
    });
    const fixture = await server((r, s) => {
      reply(s, r, { subscriptionId: "sub" });
      if (r.method === "runs.subscribe")
        notify(s, "runs.event", { subscriptionId: "sub", event: invalid });
    });
    const client = await CoreClient.connect(fixture.address);
    const feed = new RunFeed(
      client,
      "run-fixture",
      () => assert.fail("invalid event displayed"),
      fail,
    );
    try {
      await feed.start();
      assert.equal((await failure).code, "protocol");
    } finally {
      feed.dispose();
      client.close();
      await fixture.close();
    }
  }
});

const frame = (value) => JSON.stringify({ protocolVersion: "0.2", ...value }) + "\n";
const waitForClosed = async (client) => {
  const deadline = Date.now() + 2000;
  while (client.listenerCount("notification") > 0) {
    assert.ok(Date.now() < deadline, "subscription did not close");
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
};

test("foreign traffic while opening and same-chunk replay need no speculative queue", { timeout: 4000 }, async (t) => {
  const fixture = await server((r, s) => {
    if (r.method !== "runs.subscribe") return reply(s, r, {});
    const frames = [];
    for (let count = 0; count < 80; count++) {
      frames.push(frame({ method: "runs.event", params: {
        subscriptionId: "foreign", event: event(1000 + count),
      } }));
      frames.push(frame({ method: "runs.subscriptionClosed", params: {
        subscriptionId: `foreign-${count}`, reason: "shutdown", lastSequence: 0,
      } }));
    }
    frames.push(frame({ requestId: r.requestId, result: { subscriptionId: "target" } }));
    for (let sequence = 1; sequence <= 80; sequence++)
      frames.push(frame({ method: "runs.event", params: {
        subscriptionId: "target", event: event(sequence),
      } }));
    frames.push(frame({ method: "runs.subscriptionClosed", params: {
      subscriptionId: "target", reason: "run_finished", lastSequence: 80,
    } }));
    s.write(frames.join(""));
  });
  t.after(fixture.close);
  const client = await CoreClient.connect(fixture.address);
  t.after(() => client.close());
  const events = [];
  let ended;
  const complete = new Promise((resolve) => { ended = resolve; });
  const feed = new RunFeed(client, "run-fixture", (e) => {
    events.push(e.sequence);
    if (e.sequence === 80) ended();
  }, (error) => assert.fail(String(error)));
  t.after(() => feed.dispose());
  await feed.start();
  await complete;
  await waitForClosed(client);
  assert.deepEqual(events, Array.from({ length: 80 }, (_, i) => i + 1));
  assert.equal(feed.lastSequence, 80);
  assert.equal(client.listenerCount("notification"), 0);
});

test("an immediately closed empty replay is observed before start resolves", async (t) => {
  const fixture = await server((r, s) => {
    if (r.method !== "runs.subscribe") return reply(s, r, {});
    s.write(frame({ requestId: r.requestId, result: { subscriptionId: "empty" } }) +
      frame({ method: "runs.subscriptionClosed", params: {
        subscriptionId: "empty", reason: "run_finished", lastSequence: 0,
      } }));
  });
  t.after(fixture.close);
  const client = await CoreClient.connect(fixture.address);
  t.after(() => client.close());
  const feed = new RunFeed(client, "run-fixture", () => assert.fail("unexpected event"),
    (error) => assert.fail(String(error)));
  t.after(() => feed.dispose());
  await feed.start();
  await waitForClosed(client);
  assert.equal(client.listenerCount("notification"), 0);
});

test("invalid subscription acknowledgements reject without closing the client", async (t) => {
  const fixture = await server((r, s) => reply(s, r,
    r.method === "runs.subscribe" ? { subscriptionId: "" } : health));
  t.after(fixture.close);
  const client = await CoreClient.connect(fixture.address);
  t.after(() => client.close());
  const feed = new RunFeed(client, "run-fixture", () => assert.fail("unexpected event"),
    (error) => assert.fail(String(error)));
  await assert.rejects(feed.start(), (error) => error.code === "protocol");
  assert.equal(client.listenerCount("notification"), 0);
  assert.deepEqual(await client.request("core.health", {}), health);
});

test("disposing a pending subscription unsubscribes its late acknowledgement", { timeout: 4000 }, async (t) => {
  let acknowledge;
  let closed;
  const unsubscribed = new Promise((resolve) => { closed = resolve; });
  const fixture = await server((r, s) => {
    if (r.method === "runs.subscribe") {
      acknowledge = () => reply(s, r, { subscriptionId: "late" });
    } else {
      assert.equal(r.params.subscriptionId, "late");
      reply(s, r, {});
      closed();
    }
  });
  t.after(fixture.close);
  const client = await CoreClient.connect(fixture.address);
  t.after(() => client.close());
  const feed = new RunFeed(client, "run-fixture", () => assert.fail("unexpected event"),
    (error) => assert.fail(String(error)));
  const opening = feed.start();
  while (!acknowledge) await new Promise((resolve) => setImmediate(resolve));
  feed.dispose();
  acknowledge();
  await opening;
  await unsubscribed;
  assert.equal(client.listenerCount("notification"), 0);
});

test("two pending subscriptions route interleaved replay and closure independently", { timeout: 4000 }, async (t) => {
  const requests = [];
  const fixture = await server((r, s) => {
    if (r.method !== "runs.subscribe") return reply(s, r, {});
    requests.push(r);
    if (requests.length !== 2) return;
    const frames = [frame({ requestId: requests[0].requestId, result: { subscriptionId: "first" } })];
    for (let sequence = 1; sequence <= 80; sequence++)
      frames.push(frame({ method: "runs.event", params: {
        subscriptionId: "first", event: event(sequence),
      } }));
    frames.push(frame({ requestId: requests[1].requestId, result: { subscriptionId: "second" } }));
    frames.push(frame({ method: "runs.event", params: {
      subscriptionId: "second", event: event(1),
    } }));
    for (const [subscriptionId, lastSequence] of [["first", 80], ["second", 1]])
      frames.push(frame({ method: "runs.subscriptionClosed", params: {
        subscriptionId, lastSequence, reason: "run_finished",
      } }));
    s.write(frames.join(""));
  });
  t.after(fixture.close);
  const client = await CoreClient.connect(fixture.address);
  t.after(() => client.close());
  const events = [[], []];
  const feeds = events.map((received) => new RunFeed(client, "run-fixture",
    (e) => received.push(e.sequence), (error) => assert.fail(String(error))));
  t.after(() => feeds.forEach((feed) => feed.dispose()));
  await Promise.all(feeds.map((feed) => feed.start()));
  await waitForClosed(client);
  assert.deepEqual(events, [Array.from({ length: 80 }, (_, i) => i + 1), [1]]);
  assert.equal(client.listenerCount("notification"), 0);
});
