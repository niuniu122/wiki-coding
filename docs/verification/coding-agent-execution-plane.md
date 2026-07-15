# Coding Agent execution plane verification

Verified locally on 2026-07-14 using Windows x64 and Node.js v24.14.1. CI remains pinned to Node.js 20 on both Ubuntu and Windows.

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

## Security and fallback boundaries

- Discovery scans only built-in definitions and explicitly managed project/user roots. It does not scan the disk, enumerate `PATH`, crawl arbitrary `node_modules`, execute discovered code, or search online.
- v1 executors are limited to catalog metadata, bounded workspace reads, and manifest-declared npm diagnostics. There is no general shell, file-write, Git-write, install, publish, or network executor.
- Every new/resumed Session resets permission to `confirm`; model selection remains sticky in a separate user-level state file.
- Disable `capabilityEmbedding` to retain exact + BM25. Disable `agentExecution` to retain report-only catalog commands. Disable `capabilityCatalog` to return to pure chat. `/chat` is the unconditional per-request fallback.
- If capability catalog initialization itself fails, every dependent feature fails closed and the process keeps the ordinary chat route available.
- A crash with an unmatched tool request is marked `indeterminate` and is never automatically replayed. `/continue` requires an audited checkpoint, the same model, and the same capability snapshot.

## Explicit local acceptance still pending

1. Install the separately distributed, hash-pinned Granite resource package and run its real latency/quality acceptance. CI uses fake vectors and makes no claim about real Granite speed.
2. Before running `npm run smoke:provider` with the user's own key, obtain explicit authorization. The smoke result must not persist the key or raw Provider frames.

Until both optional release acceptance steps are deliberately completed, keep `agentDefaultRoute` closed and use explicit `/agent` for local trials.
