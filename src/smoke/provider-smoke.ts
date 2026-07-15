import {ApplicationKernel} from "../runtime/application-kernel.js";

if (process.argv.length > 2) {
  process.stderr.write(
    "provider connection failed: command-line credentials and arguments are not accepted\n"
  );
  process.exitCode = 1;
} else {
  const app = new ApplicationKernel({cwd: process.cwd()});
  let passed = false;

  try {
    const initEvents = await app.init();
    const ready = initEvents.find((event) => event.type === "runtime.ready");
    if (ready?.type !== "runtime.ready" || !ready.hasApiKey) {
      throw new Error("Provider credential is unavailable.");
    }

    for await (const event of app.dispatch({
      type: "turn.submit",
      input: "Reply with exactly: connected"
    })) {
      if (event.type === "error") {
        throw new Error("Provider request failed.");
      }
      if (event.type === "assistant.completed" && event.item.content.trim()) {
        passed = true;
      }
    }

    if (!passed) {
      throw new Error("Provider did not complete a visible response.");
    }
    process.stdout.write("provider connection passed\n");
  } catch {
    process.stderr.write("provider connection failed\n");
    process.exitCode = 1;
  } finally {
    await app.shutdown("user");
  }
}
