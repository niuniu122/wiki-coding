import assert from "node:assert/strict";
import test from "node:test";
import {
  CommandArbiter,
  CommandBusyError
} from "../src/runtime/command-arbiter.js";

test("only interrupt, shutdown, and read-only commands pass while a Turn runs", () => {
  const arbiter = new CommandArbiter();
  arbiter.markReady();
  const ownership = arbiter.begin({type: "turn.submit", input: "hello"});

  assert.equal(arbiter.canDispatch({type: "turn.interrupt"}), true);
  assert.equal(arbiter.canDispatch({type: "thread.list"}), true);
  assert.equal(arbiter.canDispatch({type: "provider.list"}), true);
  assert.equal(arbiter.canDispatch({type: "trace.toggle"}), true);
  assert.equal(arbiter.canDispatch({type: "app.exit"}), true);
  assert.equal(arbiter.canDispatch({type: "thread.new"}), false);
  assert.equal(
    arbiter.canDispatch({type: "provider.switch", providerId: "hashsight"}),
    false
  );

  ownership.finish();
  assert.equal(arbiter.canDispatch({type: "thread.new"}), true);
});

test("begin rejects a concurrent mutating command with a typed busy error", () => {
  const arbiter = new CommandArbiter();
  arbiter.markReady();
  const ownership = arbiter.begin({type: "turn.submit", input: "hello"});

  assert.throws(
    () => arbiter.begin({type: "thread.new"}),
    (error) =>
      error instanceof CommandBusyError &&
      error.phase === "running_turn" &&
      error.commandType === "thread.new"
  );

  ownership.finish();
});

test("every idle mutation owns the runtime until it finishes", () => {
  const arbiter = new CommandArbiter();
  arbiter.markReady();

  const mutations = [
    {type: "thread.new"} as const,
    {type: "thread.resume", threadId: "thread_1"} as const,
    {type: "turn.submit", input: "hello"} as const,
    {type: "compact.manual"} as const,
    {type: "config.api_key.request"} as const,
    {type: "config.api_key.plaintext.confirm"} as const,
    {type: "config.api_key.set", apiKey: "secret"} as const,
    {type: "provider.switch", providerId: "minimax-official"} as const
  ];

  for (const command of mutations) {
    const ownership = arbiter.begin(command);
    assert.throws(
      () => arbiter.begin({type: "thread.new"}),
      (error) => error instanceof CommandBusyError
    );
    assert.equal(arbiter.canDispatch({type: "thread.list"}), true);
    ownership.finish();
    assert.equal(arbiter.canDispatch({type: "thread.new"}), true);
  }
});

test("finishing a concurrent read-only command does not finish the active Turn", () => {
  const arbiter = new CommandArbiter();
  arbiter.markReady();
  const turn = arbiter.begin({type: "turn.submit", input: "hello"});

  const listing = arbiter.begin({type: "thread.list"});
  listing.finish();

  assert.equal(arbiter.canDispatch({type: "thread.new"}), false);
  turn.finish();
  assert.equal(arbiter.canDispatch({type: "thread.new"}), true);
});

test("shutdown stops accepting new commands", () => {
  const arbiter = new CommandArbiter();
  arbiter.markReady();

  arbiter.beginShutdown();

  assert.equal(arbiter.canDispatch({type: "thread.list"}), false);
  assert.throws(
    () => arbiter.begin({type: "thread.list"}),
    (error) =>
      error instanceof CommandBusyError && error.phase === "shutting_down"
  );
});

test("markReady cannot release an active Turn", () => {
  const arbiter = new CommandArbiter();
  arbiter.markReady();
  const ownership = arbiter.begin({type: "turn.submit", input: "hello"});

  arbiter.markReady();

  assert.equal(arbiter.canDispatch({type: "thread.new"}), false);
  ownership.finish();
});

test("markReady cannot restart a shutting-down kernel", () => {
  const arbiter = new CommandArbiter();
  arbiter.markReady();
  arbiter.beginShutdown();

  arbiter.markReady();

  assert.equal(arbiter.canDispatch({type: "thread.list"}), false);
});

test("booting allows only app exit", () => {
  const arbiter = new CommandArbiter();

  assert.equal(arbiter.canDispatch({type: "app.exit"}), true);
  for (const command of [
    {type: "thread.list"} as const,
    {type: "provider.list"} as const,
    {type: "thread.new"} as const,
    {type: "config.api_key.request"} as const
  ]) {
    assert.equal(arbiter.canDispatch(command), false, command.type);
  }
  arbiter.beginShutdown();
  assert.equal(arbiter.canDispatch({type: "app.exit"}), false);
  arbiter.markStopped();
  assert.equal(arbiter.canDispatch({type: "app.exit"}), false);
});
