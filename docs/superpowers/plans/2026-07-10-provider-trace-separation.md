# Provider and Trace Separation Plan

**Status:** Implemented and verified

## Goal

Split the current all-in-one model adapter into transport, wire-protocol,
provider-error, reasoning-filter, and runtime trace boundaries. Preserve the
existing MiniMax/OpenAI-compatible behavior while ensuring raw model reasoning,
request prompts, and credentials cannot enter durable trace events.

## Implementation slices

1. Define failing tests for protocol ownership, safe reasoning filtering,
   structured provider errors, and trace fact allowlists.
2. Extract HTTP transport and Responses/Chat Completions protocol modules.
3. Introduce a provider-neutral model adapter that composes those modules.
4. Replace arbitrary trace label/content writes with known trace codes and
   per-code fact allowlists.
5. Run offline tests, type checking, and production build; update architecture
   documentation and the execution log.

## Invariants

- Provider request bodies may contain the model context, but trace events may not.
- Raw reasoning fields and `<think>` blocks are discarded before Runtime events.
- Upstream errors become structured categories; secrets echoed by an upstream
  response are redacted before reaching UI or storage.
- External abort remains distinguishable from transport timeout.
- No real provider request or credential is required for verification.

## Result

- Added separate protocol, HTTP transport, provider error, provider adapter,
  reasoning filter, and safe trace recorder modules.
- Kept the legacy `MiniMaxModelAdapter` export as an alias while Runtime now
  defaults to the provider-neutral adapter.
- Removed the arbitrary model `trace` event channel.
- Added full-stream timeout coverage so a stalled body is still cancelled after
  response headers have arrived.
- Verification: 49 tests passed, `npm run check` passed, and `npm run build`
  passed without using a real provider credential.
