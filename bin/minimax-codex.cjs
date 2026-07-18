#!/usr/bin/env node
"use strict";

const {closeSync, lstatSync, openSync, readSync} = require("node:fs");
const {join} = require("node:path");
const {spawnSync} = require("node:child_process");

const targetKey = `${process.platform}:${process.arch}`;
const packagedBinary = Object.freeze({
  "win32:x64": "minimax-codex.exe",
  "linux:x64": "minimax-codex"
})[targetKey];

if (!packagedBinary) {
  fail(
    "E_UNSUPPORTED_HOST",
    `Unsupported host: ${process.platform}/${process.arch}. Expected packaged targets: win32/x64, linux/x64.`
  );
}

const binaryPath = join(__dirname, "..", packagedBinary);
let status;
try {
  status = lstatSync(binaryPath);
} catch {
  fail(
    "E_BINARY_MISSING",
    `The packaged Rust binary is missing for ${targetKey}.`,
    binaryPath
  );
}
if (!status.isFile() || status.isSymbolicLink()) {
  fail(
    "E_BINARY_UNSAFE",
    `The packaged Rust binary is not a safe regular file for ${targetKey}.`,
    binaryPath
  );
}
if (process.platform !== "win32" && (status.mode & 0o111) === 0) {
  fail(
    "E_BINARY_NOT_EXECUTABLE",
    `The packaged Rust binary is not executable for ${targetKey}.`,
    binaryPath
  );
}

const expectedMagic = targetKey === "win32:x64"
  ? Buffer.from([0x4d, 0x5a])
  : Buffer.from([0x7f, 0x45, 0x4c, 0x46]);
let binaryMagic;
try {
  const descriptor = openSync(binaryPath, "r");
  try {
    binaryMagic = Buffer.alloc(expectedMagic.length);
    if (readSync(descriptor, binaryMagic, 0, binaryMagic.length, 0) !== binaryMagic.length) {
      binaryMagic = undefined;
    }
  } finally {
    closeSync(descriptor);
  }
} catch {
  binaryMagic = undefined;
}
if (!binaryMagic || !binaryMagic.equals(expectedMagic)) {
  fail(
    "E_START_FAILED",
    `The packaged Rust binary could not start for ${targetKey}: invalid executable format.`,
    binaryPath
  );
}

const result = spawnSync(binaryPath, process.argv.slice(2), {
  stdio: "inherit",
  shell: false,
  windowsHide: true
});
if (result.error) {
  fail(
    "E_START_FAILED",
    `The packaged Rust binary could not start for ${targetKey}: ${result.error.code ?? "unknown error"}.`,
    binaryPath
  );
}
if (result.status === null) {
  fail(
    "E_SIGNAL_TERMINATION",
    `The packaged Rust binary ended by signal for ${targetKey}: ${result.signal ?? "unknown"}.`,
    binaryPath
  );
}
process.exitCode = result.status;

function fail(code, message, expectedPath) {
  const pathGuidance = expectedPath ? ` Expected path: ${expectedPath}.` : "";
  process.stderr.write(
    `minimax-codex [${code}]: ${message}${pathGuidance} Reinstall minimax-codex for a supported Windows x64 or Linux x64 release.\n`
  );
  process.exit(1);
}
