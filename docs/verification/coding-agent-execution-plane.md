# Coding Agent execution plane verification

Originally verified locally on 2026-07-14 using Windows x64 and Node.js v24.14.1. The repaired full-access Shell contract was verified locally on 2026-07-22 with Rust 1.97 on Windows. CI remains pinned to Node.js 20 and is configured to run the native Shell I/O suite on Ubuntu and Windows; fresh hosted Windows/Linux evidence is still pending.

## Current outcome

The implementation is complete behind independent feature flags. Ordinary text remains chat by default. Explicit Agent execution is available only when `capabilityCatalog` and `agentExecution` are both enabled. `/chat` always bypasses Agent routing.

The release gate for `agentDefaultRoute` remains closed in production. This is intentional: no Granite resource package is installed in this workspace, and no live Provider smoke was authorized. Neither missing optional acceptance step is reported as passed.

Example explicit, reversible configuration:

```json
{
  "features": {
    "capabilityCatalog": true,
    "capabilityEmbedding": false,
    "agentExecution": true,
    "agentDefaultRoute": false
  }
}
```

The four fields are optional. A missing or legacy `features` object is not written automatically and preserves the old chat route.

## Offline commands and results

```text
npm test                  426/426 passed at the final offline gate
npm run check             passed
npm run build             passed
npm run eval:retrieval    175 curated cases; all report gates passed
npm run eval:provider     8/8 checks for each built-in protocol passed
git diff --check          passed (line-ending notices only)
```

The retrieval report separately records lexical, deterministic fake-vector embedding, fused, and no-resource fallback metrics. On this reference host all lexical/fused/fallback metrics were 1.0; deterministic embedding recall/top-1/MRR were about 0.9852; ID validity and no-match precision were 1.0. Measured p95 was approximately 0.0067 ms for exact lookup, 0.0186 ms for warm lexical lookup, and 0.0009 ms for the disabled chat route. These timings are host observations, not universal promises.

Provider conformance is fully offline and covers request shape, streaming text, tool-call assembly, usage, terminal completion, cancellation, malformed/premature streams, normalized failure, redaction, and unsupported-feature fail-closed behavior for Responses and Chat Completions.

CI runs only dependency installation, type checking, tests, build, and the two deterministic offline reports. It does not call the live smoke command, reference a real credential, download a model, or require an embedding package.

## Full-access Shell verification

The runtime has exactly two permission modes. A new process starts in
`confirm`, where arbitrary Shell is hidden and rejected. `full-access` exposes
`shell_command` plus `shell_session` and runs them without per-command
confirmation. Ordinary calls default to lossless pipe capture; `tty: true`
opts into a fixed 120x30 PTY/ConPTY and terminal wrapping. The real Shell I/O
suite covers both modes, fast and exit-7 commands, incremental long output,
prompt input, TTY detection, default/relative/outside working directories,
Unicode and emoji, native pipes and redirects, the 1 MiB unread limit, explicit
stop, permission downgrade, and normal shutdown. Parent and child process IDs
are printed by cleanup fixtures and checked for survivors.

Both modes retain process-scoped sessions plus bounded stdin, output, polling,
and cleanup. Shell output is a bounded ordinary tool result. It is persisted
locally and sent to the configured Provider; command text, input, working
directory, and output are not copied into safe trace metadata. Windows command
payloads up to 32 KiB are delivered outside PowerShell argv and acknowledged
before the Shell host reports readiness. Normal stop/downgrade/shutdown cleanup
is tested, but a forced application kill or machine/OS failure cannot guarantee
cleanup. macOS support, terminal resizing, browser control, Pi, a Node Agent
runtime, tmux, push, release, and an external terminal remain outside this
change.

## Security and fallback boundaries

- Discovery scans only built-in definitions and explicitly managed project/user roots. It does not scan the disk, enumerate `PATH`, crawl arbitrary `node_modules`, execute discovered code, or search online.
- In `confirm`, executors remain limited to the original bounded tools and arbitrary Shell is unavailable. In explicitly selected, process-scoped `full-access`, the two Shell tools are a general host command executor and can perform file, Git, install, publish, or network operations with the launching user's rights.
- Every new/resumed Session resets permission to `confirm`; model selection remains sticky in a separate user-level state file.
- Disable `capabilityEmbedding` to retain exact + BM25. Disable `agentExecution` to retain report-only catalog commands. Disable `capabilityCatalog` to return to pure chat. `/chat` is the unconditional per-request fallback.
- If capability catalog initialization itself fails, every dependent feature fails closed and the process keeps the ordinary chat route available.
- A crash with an unmatched tool request is marked `indeterminate` and is never automatically replayed. `/continue` requires an audited checkpoint, the same model, and the same capability snapshot.

## Explicit local acceptance still pending

1. Install the separately distributed, hash-pinned Granite resource package and run its real latency/quality acceptance. CI uses fake vectors and makes no claim about real Granite speed.
2. Before running `npm run smoke:provider` with the user's own key, obtain explicit authorization. The smoke result must not persist the key or raw Provider frames.

Until both optional release acceptance steps are deliberately completed, keep `agentDefaultRoute` closed and use explicit `/agent` for local trials.
