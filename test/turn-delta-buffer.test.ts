import assert from "node:assert/strict";
import test from "node:test";
import {TurnDeltaBuffer} from "../src/storage/turn-delta-buffer.js";

test("delta buffer flushes at 1024 characters and on force flush", async () => {
  const batches: Array<{delta: string; createdAt: string}> = [];
  const buffer = new TurnDeltaBuffer(
    async (delta, createdAt) => {
      batches.push({delta, createdAt});
    },
    {delayMs: 250, maxCharacters: 1024}
  );

  await buffer.push("x".repeat(1024));
  await buffer.push("tail");
  await buffer.flush();

  assert.deepEqual(
    batches.map((batch) => batch.delta),
    ["x".repeat(1024), "tail"]
  );
  assert.ok(batches.every((batch) => !Number.isNaN(Date.parse(batch.createdAt))));
  await buffer.close();
});

test("delta buffer flushes after its delay from the first pending delta", async () => {
  let resolvePersisted!: (value: string) => void;
  const persisted = new Promise<string>((resolve) => {
    resolvePersisted = resolve;
  });
  const buffer = new TurnDeltaBuffer(
    async (delta) => {
      resolvePersisted(delta);
    },
    {delayMs: 20, maxCharacters: 1024}
  );

  await buffer.push("timed");

  assert.equal(await persisted, "timed");
  await buffer.close();
});

test("closing force-flushes pending text and rejects later pushes", async () => {
  const batches: string[] = [];
  const buffer = new TurnDeltaBuffer(async (delta) => {
    batches.push(delta);
  });

  await buffer.push("terminal partial");
  await buffer.close();

  assert.deepEqual(batches, ["terminal partial"]);
  await assert.rejects(buffer.push("too late"), /closed/i);
});

test("a failed timer flush is observed once and re-arms a sub-threshold retry", async () => {
  let attempts = 0;
  let firstAttempt!: () => void;
  let retried!: (delta: string) => void;
  const firstAttempted = new Promise<void>((resolve) => {
    firstAttempt = resolve;
  });
  const retriedDelta = new Promise<string>((resolve) => {
    retried = resolve;
  });
  const buffer = new TurnDeltaBuffer(
    async (delta) => {
      attempts += 1;
      if (attempts === 1) {
        firstAttempt();
        throw new Error("transient persist failure");
      }
      retried(delta);
    },
    {delayMs: 20, maxCharacters: 1024}
  );

  await buffer.push("pending ");
  await firstAttempted;
  await new Promise<void>((resolve) => setImmediate(resolve));
  await assert.rejects(buffer.push("tail"), /transient persist failure/i);
  await buffer.push("tail");

  const persisted = await Promise.race([
    retriedDelta,
    new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error("timer retry did not run")), 250)
    )
  ]);
  assert.equal(persisted, "pending tail");
  assert.equal(attempts, 2);
  await buffer.close();
});

test("explicit flush immediately retries retained text after a timer failure", async () => {
  let attempts = 0;
  let firstAttempt!: () => void;
  const firstAttempted = new Promise<void>((resolve) => {
    firstAttempt = resolve;
  });
  const persisted: string[] = [];
  const buffer = new TurnDeltaBuffer(
    async (delta) => {
      attempts += 1;
      if (attempts === 1) {
        firstAttempt();
        throw new Error("timer persist failure");
      }
      persisted.push(delta);
    },
    {delayMs: 20, maxCharacters: 1024}
  );

  await buffer.push("flush me");
  await firstAttempted;
  await new Promise<void>((resolve) => setImmediate(resolve));

  await buffer.flush();
  await buffer.flush();

  assert.deepEqual(persisted, ["flush me"]);
  assert.equal(attempts, 2);
  await buffer.close();
});

test("close immediately retries retained text after a timer failure", async () => {
  let attempts = 0;
  let firstAttempt!: () => void;
  const firstAttempted = new Promise<void>((resolve) => {
    firstAttempt = resolve;
  });
  const persisted: string[] = [];
  const buffer = new TurnDeltaBuffer(
    async (delta) => {
      attempts += 1;
      if (attempts === 1) {
        firstAttempt();
        throw new Error("timer persist failure");
      }
      persisted.push(delta);
    },
    {delayMs: 20, maxCharacters: 1024}
  );

  await buffer.push("close me");
  await firstAttempted;
  await new Promise<void>((resolve) => setImmediate(resolve));

  await buffer.close();
  await buffer.close();

  assert.deepEqual(persisted, ["close me"]);
  assert.equal(attempts, 2);
  await assert.rejects(buffer.push("too late"), /closed/i);
});

test("a failed forced flush rejects without losing its retryable batch", async () => {
  let attempts = 0;
  let firstAttempt!: () => void;
  const firstAttempted = new Promise<void>((resolve) => {
    firstAttempt = resolve;
  });
  const persisted: string[] = [];
  const buffer = new TurnDeltaBuffer(
    async (delta) => {
      attempts += 1;
      if (attempts === 1) {
        firstAttempt();
        throw new Error("timer persist failure");
      }
      if (attempts === 2) {
        throw new Error("forced flush failure");
      }
      persisted.push(delta);
    },
    {delayMs: 20, maxCharacters: 1024}
  );

  await buffer.push("retry flush");
  await firstAttempted;
  await new Promise<void>((resolve) => setImmediate(resolve));

  await assert.rejects(buffer.flush(), /forced flush failure/i);
  assert.equal(attempts, 2);
  await buffer.flush();

  assert.deepEqual(persisted, ["retry flush"]);
  assert.equal(attempts, 3);
  await buffer.close();
});

test("a failed forced close rejects without losing its retryable batch", async () => {
  let attempts = 0;
  let firstAttempt!: () => void;
  const firstAttempted = new Promise<void>((resolve) => {
    firstAttempt = resolve;
  });
  const persisted: string[] = [];
  const buffer = new TurnDeltaBuffer(
    async (delta) => {
      attempts += 1;
      if (attempts === 1) {
        firstAttempt();
        throw new Error("timer persist failure");
      }
      if (attempts === 2) {
        throw new Error("forced close failure");
      }
      persisted.push(delta);
    },
    {delayMs: 20, maxCharacters: 1024}
  );

  await buffer.push("retry close");
  await firstAttempted;
  await new Promise<void>((resolve) => setImmediate(resolve));

  await assert.rejects(buffer.close(), /forced close failure/i);
  assert.equal(attempts, 2);
  await buffer.close();

  assert.deepEqual(persisted, ["retry close"]);
  assert.equal(attempts, 3);
  await assert.rejects(buffer.push("too late"), /closed/i);
});
