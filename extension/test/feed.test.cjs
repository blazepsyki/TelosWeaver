const { test } = require("node:test");
const assert = require("node:assert/strict");
const { CoreClient } = require("../dist/client");
const { RunFeed } = require("../dist/runFeed");
const { server, reply, notify } = require("./helpers.cjs");
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
