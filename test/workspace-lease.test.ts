import assert from "node:assert/strict";
import {mkdirSync, rmSync, writeFileSync} from "node:fs";
import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rename,
  rm,
  writeFile
} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {WorkspaceLease} from "../src/runtime/workspace-lease.js";

async function withWorkspace(
  run: (root: string) => Promise<void>
): Promise<void> {
  const root = await mkdtemp(join(tmpdir(), "minimax-workspace-lease-"));
  try {
    await run(root);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
}

function deferred<T = void>(): {
  promise: Promise<T>;
  resolve(value?: T): void;
} {
  let resolvePromise!: (value: T) => void;
  const promise = new Promise<T>((resolver) => {
    resolvePromise = resolver;
  });
  return {
    promise,
    resolve: (value?: T) => resolvePromise(value as T)
  };
}

test("a second live owner cannot acquire the same workspace", async () => {
  await withWorkspace(async (root) => {
    const first = new WorkspaceLease(root, {
      pid: 100,
      isProcessAlive: () => true
    });
    const second = new WorkspaceLease(root, {
      pid: 200,
      isProcessAlive: () => true
    });

    await first.acquire();

    await assert.rejects(() => second.acquire(), /already open.*PID 100/i);
  });
});

test("a dead owner is replaced without deleting the new owner", async () => {
  await withWorkspace(async (root) => {
    const stale = new WorkspaceLease(root, {
      pid: 100,
      isProcessAlive: () => false
    });
    await stale.acquire();

    const recovered = new WorkspaceLease(root, {
      pid: 200,
      isProcessAlive: () => false
    });
    await recovered.acquire();
    await stale.release();

    await assert.rejects(
      () =>
        new WorkspaceLease(root, {
          pid: 300,
          isProcessAlive: () => true
        }).acquire(),
      /PID 200/
    );
  });
});

test("invalid owner metadata is recovered as stale ownership", async () => {
  await withWorkspace(async (root) => {
    const lockDir = join(root, "locks", "runtime.lock");
    await mkdir(lockDir, {recursive: true});
    await writeFile(join(lockDir, "owner.json"), "{not-json", "utf8");

    const lease = new WorkspaceLease(root, {
      pid: 400,
      now: () => "2026-07-10T00:00:00.000Z"
    });
    await lease.acquire();

    const owner = JSON.parse(
      await readFile(join(lockDir, "owner.json"), "utf8")
    ) as {pid: number; startedAt: string; workspace: string; nonce: string};
    assert.equal(owner.pid, 400);
    assert.equal(owner.startedAt, "2026-07-10T00:00:00.000Z");
    assert.equal(owner.workspace, root);
    assert.equal(typeof owner.nonce, "string");
  });
});

test("the current owner can release and reacquire the workspace", async () => {
  await withWorkspace(async (root) => {
    const first = new WorkspaceLease(root, {pid: 500});
    await first.acquire();
    await first.release();

    const second = new WorkspaceLease(root, {pid: 600});
    await second.acquire();
    await second.release();
  });
});

test("stale recovery revalidates a replacement owner before renaming", async () => {
  await withWorkspace(async (root) => {
    const lockDir = join(root, "locks", "runtime.lock");
    const stale = new WorkspaceLease(root, {pid: 100});
    await stale.acquire();

    let replacementInstalled = false;
    const contender = new WorkspaceLease(root, {
      pid: 300,
      isProcessAlive: (pid) => {
        if (pid === 100 && !replacementInstalled) {
          replacementInstalled = true;
          rmSync(lockDir, {recursive: true, force: true});
          mkdirSync(lockDir, {recursive: true});
          writeFileSync(
            join(lockDir, "owner.json"),
            `${JSON.stringify({
              pid: 200,
              startedAt: "2026-07-10T01:00:00.000Z",
              workspace: root,
              nonce: "replacement-owner"
            })}\n`,
            "utf8"
          );
          return false;
        }
        return pid === 200;
      }
    });

    await assert.rejects(() => contender.acquire(), /already open.*PID 200/i);

    const owner = JSON.parse(
      await readFile(join(lockDir, "owner.json"), "utf8")
    ) as {pid: number; nonce: string};
    assert.equal(owner.pid, 200);
    assert.equal(owner.nonce, "replacement-owner");
  });
});

test("concurrent stale recovery allows only one live owner", async () => {
  await withWorkspace(async (root) => {
    const stale = new WorkspaceLease(root, {pid: 100});
    await stale.acquire();

    const isProcessAlive = (pid: number): boolean => pid !== 100;
    const first = new WorkspaceLease(root, {pid: 200, isProcessAlive});
    const second = new WorkspaceLease(root, {pid: 300, isProcessAlive});
    const results = await Promise.allSettled([first.acquire(), second.acquire()]);

    assert.equal(
      results.filter((result) => result.status === "fulfilled").length,
      1
    );
    const rejected = results.find((result) => result.status === "rejected");
    assert.equal(rejected?.status, "rejected");
    if (rejected?.status === "rejected") {
      assert.match(String(rejected.reason), /already open.*PID (200|300)/i);
    }
  });
});

test("concurrent releases share one operation and preserve a replacement owner", async () => {
  await withWorkspace(async (root) => {
    const lease = new WorkspaceLease(root, {pid: 700});
    await lease.acquire();

    const firstRelease = lease.release();
    const secondRelease = lease.release();
    assert.strictEqual(secondRelease, firstRelease);
    await firstRelease;

    const replacement = new WorkspaceLease(root, {pid: 800});
    await replacement.acquire();
    await secondRelease;

    await assert.rejects(
      () =>
        new WorkspaceLease(root, {
          pid: 900,
          isProcessAlive: () => true
        }).acquire(),
      /already open.*PID 800/i
    );
  });
});

test("owner metadata is complete before delayed atomic publication", async () => {
  await withWorkspace(async (root) => {
    const lockDir = join(root, "locks", "runtime.lock");
    const publicationStarted = deferred<"publication-started">();
    const allowPublication = deferred();
    let intercepted = false;
    const first = new WorkspaceLease(root, {
      pid: 100,
      isProcessAlive: (pid) => pid === 200,
      fileOperations: {
        rename: async (from, to) => {
          if (!intercepted && to === lockDir && from.includes(".candidate.")) {
            intercepted = true;
            publicationStarted.resolve("publication-started");
            await allowPublication.promise;
          }
          await rename(from, to);
        }
      }
    });
    const firstAcquire = first.acquire();
    const firstSettled = firstAcquire.then(
      () => "settled" as const,
      () => "settled" as const
    );

    const firstSignal = await Promise.race([
      publicationStarted.promise,
      firstSettled
    ]);
    assert.equal(firstSignal, "publication-started");

    const second = new WorkspaceLease(root, {pid: 200});
    await second.acquire();
    const publishedOwner = JSON.parse(
      await readFile(join(lockDir, "owner.json"), "utf8")
    ) as {pid: number; nonce: string};
    assert.equal(publishedOwner.pid, 200);
    assert.equal(publishedOwner.nonce.length > 0, true);

    allowPublication.resolve();
    await assert.rejects(firstAcquire, /already open.*PID 200/i);
  });
});

test("an orphaned operation authority is recovered without blocking acquisition", async () => {
  await withWorkspace(async (root) => {
    const stale = new WorkspaceLease(root, {pid: 100});
    await stale.acquire();
    const staleOwner = JSON.parse(
      await readFile(join(root, "locks", "runtime.lock", "owner.json"), "utf8")
    ) as {nonce: string};

    const operationDir = join(root, "locks", "runtime.lock.operation");
    await mkdir(operationDir);
    await writeFile(
      join(operationDir, "owner.json"),
      `${JSON.stringify({
        kind: "workspace_operation",
        pid: 400,
        startedAt: "2026-07-10T02:00:00.000Z",
        workspace: root,
        nonce: "orphaned-operation",
        targetIdentity: `owner:${staleOwner.nonce}`
      })}\n`,
      "utf8"
    );

    const recovered = new WorkspaceLease(root, {
      pid: 200,
      isProcessAlive: () => false
    });
    await recovered.acquire();

    const owner = JSON.parse(
      await readFile(join(root, "locks", "runtime.lock", "owner.json"), "utf8")
    ) as {pid: number};
    assert.equal(owner.pid, 200);
    await assert.rejects(access(operationDir), {code: "ENOENT"});
  });
});

test("a transient release failure preserves ownership for a successful retry", async () => {
  await withWorkspace(async (root) => {
    const lockDir = join(root, "locks", "runtime.lock");
    let failRemoval = true;
    const lease = new WorkspaceLease(root, {
      pid: 500,
      fileOperations: {
        remove: async (path) => {
          if (path === lockDir && failRemoval) {
            failRemoval = false;
            throw Object.assign(new Error("transient remove failure"), {
              code: "EBUSY"
            });
          }
          await rm(path, {recursive: true, force: true});
        }
      }
    });
    await lease.acquire();

    await assert.rejects(lease.release(), /transient remove failure/);
    await lease.release();

    const next = new WorkspaceLease(root, {pid: 600});
    await next.acquire();
    await next.release();
  });
});

test("a canonical workspace directory without owner metadata is recovered", async () => {
  await withWorkspace(async (root) => {
    const lockDir = join(root, "locks", "runtime.lock");
    await mkdir(lockDir, {recursive: true});
    let publishAttempts = 0;
    const lease = new WorkspaceLease(root, {
      pid: 700,
      fileOperations: {
        rename: async (from, to) => {
          if (to === lockDir && from.includes(".candidate.")) {
            publishAttempts += 1;
            if (publishAttempts === 1) {
              throw Object.assign(new Error("simulated existing lock directory"), {
                code: "EEXIST"
              });
            }
            if (publishAttempts > 2) {
              await rm(lockDir, {recursive: true, force: true});
              throw new Error("workspace publication loop detected");
            }
          }
          await rename(from, to);
        }
      }
    });

    await lease.acquire();

    const owner = JSON.parse(
      await readFile(join(lockDir, "owner.json"), "utf8")
    ) as {pid: number};
    assert.equal(owner.pid, 700);
    assert.equal(publishAttempts, 2);
  });
});

test("an operation authority without owner metadata is recovered", async () => {
  await withWorkspace(async (root) => {
    const stale = new WorkspaceLease(root, {pid: 100});
    await stale.acquire();

    const lockDir = join(root, "locks", "runtime.lock");
    const operationDir = `${lockDir}.operation`;
    await mkdir(operationDir);
    let authorityPublishAttempts = 0;
    const recovered = new WorkspaceLease(root, {
      pid: 800,
      isProcessAlive: () => false,
      fileOperations: {
        rename: async (from, to) => {
          if (to === operationDir && from.includes(".candidate.")) {
            authorityPublishAttempts += 1;
            if (authorityPublishAttempts === 1) {
              throw Object.assign(
                new Error("simulated existing operation authority"),
                {code: "EEXIST"}
              );
            }
            if (authorityPublishAttempts > 2) {
              await rm(operationDir, {recursive: true, force: true});
              throw new Error("authority publication loop detected");
            }
          }
          await rename(from, to);
        }
      }
    });

    await recovered.acquire();

    const owner = JSON.parse(
      await readFile(join(lockDir, "owner.json"), "utf8")
    ) as {pid: number};
    assert.equal(owner.pid, 800);
    assert.equal(authorityPublishAttempts, 2);
  });
});

test("authority cleanup completes after its release rename fails", async () => {
  await withWorkspace(async (root) => {
    const lockDir = join(root, "locks", "runtime.lock");
    const operationDir = `${lockDir}.operation`;
    let failAuthorityRelease = true;
    const lease = new WorkspaceLease(root, {
      pid: 900,
      fileOperations: {
        rename: async (from, to) => {
          if (
            from === operationDir &&
            to.includes(".released.") &&
            failAuthorityRelease
          ) {
            failAuthorityRelease = false;
            throw Object.assign(new Error("transient authority release failure"), {
              code: "EBUSY"
            });
          }
          await rename(from, to);
        }
      }
    });
    await lease.acquire();

    await lease.release();

    const next = new WorkspaceLease(root, {
      pid: 901,
      isProcessAlive: (pid) => pid === 900
    });
    await next.acquire();
    await next.release();
    await assert.rejects(access(operationDir), {code: "ENOENT"});
  });
});

test("owner preparation failure removes its UUID candidate directory", async () => {
  await withWorkspace(async (root) => {
    const lease = new WorkspaceLease(root, {
      pid: 950,
      fileOperations: {
        writeOwner: async () => {
          throw new Error("owner preparation failed");
        }
      }
    });

    await assert.rejects(lease.acquire(), /owner preparation failed/);

    const locksDir = join(root, "locks");
    const entries = await readdir(locksDir);
    assert.equal(entries.some((entry) => entry.includes(".candidate.")), false);
  });
});
