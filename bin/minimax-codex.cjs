#!/usr/bin/env node
"use strict";

const {lstatSync} = require("node:fs");
const {join} = require("node:path");
const {spawnSync} = require("node:child_process");

const packagedBinary = Object.freeze({
  "win32:x64": "minimax-codex.exe",
  "linux:x64": "minimax-codex"
})[`${process.platform}:${process.arch}`];

if (!packagedBinary) {
  fail(`Unsupported platform/architecture: ${process.platform}/${process.arch}.`);
}

const binaryPath = join(__dirname, "..", packagedBinary);
let status;
try {
  status = lstatSync(binaryPath);
} catch {
  fail(`The packaged Rust binary is missing: ${packagedBinary}.`);
}
if (!status.isFile() || status.isSymbolicLink()) {
  fail(`The packaged Rust binary is not a safe regular file: ${packagedBinary}.`);
}
if (process.platform !== "win32" && (status.mode & 0o111) === 0) {
  fail(`The packaged Rust binary is not executable: ${packagedBinary}.`);
}

const result = spawnSync(binaryPath, process.argv.slice(2), {
  stdio: "inherit",
  shell: false,
  windowsHide: true
});
if (result.error) {
  fail(`The packaged Rust binary could not start: ${result.error.code ?? "unknown error"}.`);
}
if (result.status === null) {
  fail(`The packaged Rust binary ended by signal: ${result.signal ?? "unknown"}.`);
}
process.exitCode = result.status;

function fail(message) {
  process.stderr.write(
    `${message} Reinstall minimax-codex for a supported Windows x64 or Linux x64 release.\n`
  );
  process.exit(1);
}
