#!/usr/bin/env node
import React from "react";
import {render} from "ink";
import {fileURLToPath} from "node:url";
import {resolve} from "node:path";
import {CommandDispatcher} from "./runtime/command-dispatcher.js";
import type {RuntimeApplication} from "./runtime/runtime-application.js";
import {
  ShutdownCoordinator,
  type SignalSource
} from "./runtime/shutdown-coordinator.js";
import {App} from "./ui/App.js";

export type RuntimeDispatcher = RuntimeApplication;

export interface CliRenderInstance {
  waitUntilExit(): Promise<void>;
  unmount(): void;
}

export interface RunCliOptions {
  dispatcher?: RuntimeDispatcher;
  signalSource?: SignalSource;
  renderApp?(dispatcher: RuntimeDispatcher): CliRenderInstance;
  writeError?(message: string): void;
  setExitCode?(code: number): void;
}

export async function runCli(options: RunCliOptions = {}): Promise<void> {
  const dispatcher = options.dispatcher ?? new CommandDispatcher();
  const instance = options.renderApp
    ? options.renderApp(dispatcher)
    : render(<App dispatcher={dispatcher} />);
  const coordinator = new ShutdownCoordinator({
    source: options.signalSource ?? process,
    shutdown: (reason) => dispatcher.shutdown(reason),
    afterShutdown: () => instance.unmount()
  });
  coordinator.install();

  try {
    await instance.waitUntilExit();
    await coordinator.request("user");
  } finally {
    coordinator.uninstall();
  }
}

export async function runCliMain(options: RunCliOptions = {}): Promise<void> {
  try {
    await runCli(options);
  } catch (error) {
    const message = `${error instanceof Error ? error.message : String(error)}\n`;
    (options.writeError ?? ((value: string) => process.stderr.write(value)))(message);
    (options.setExitCode ?? ((code: number) => {
      process.exitCode = code;
    }))(1);
  }
}

function isEntrypoint(): boolean {
  const entry = process.argv[1];
  return entry !== undefined && resolve(entry) === resolve(fileURLToPath(import.meta.url));
}

if (isEntrypoint()) {
  void runCliMain();
}
