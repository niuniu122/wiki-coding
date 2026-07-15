---
status: testing
phase: MMX-01-contract-foundation
source: [01-VERIFICATION.md]
started: 2026-07-15T09:31:25Z
updated: 2026-07-15T09:31:25Z
---

## Current Test

number: 1
name: Hosted Windows/MSVC and Linux CI
expected: |
  Both ubuntu-latest and windows-latest pass all twelve offline CI steps with no credentials, live Provider calls, or embedding downloads.
awaiting: user response

## Tests

### 1. Hosted Windows/MSVC and Linux CI

expected: Both matrix jobs pass npm install/check/test, pinned Rust install/fmt/Clippy/tests/contracts, build, and both offline evaluations.
result: pending

## Summary

total: 1
passed: 0
issues: 0
pending: 1
skipped: 0
blocked: 0

## Gaps
