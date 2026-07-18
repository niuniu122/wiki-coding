# Hosted product fingerprint EOL drift

## Status

Resolved locally on 2026-07-18. A fresh hosted candidate run is still required.

## Evidence

- Candidate workflow run: `29635653025`
- Windows job `88057410484`: success, fingerprint `3e89d8dd0dace9681d7186e03ea41c94e857aec07282720b8387b8735abec2a2`
- Linux job `88057410487`: success, fingerprint `fd43ad92893380002ece3e428c50f9c091e7a713f5c150f0af6ddfb973a9303f`
- Both jobs reported 235 product inputs, the exact candidate tree, offline operation, and zero Provider, credential, or model-download activity.
- The run is operationally green but cannot become combined candidate evidence because schema v2 requires one current product fingerprint shared by both hosted jobs.

## Root cause

Product fingerprint v3 intentionally hashes current working-tree bytes so uncommitted product edits cannot reuse an index snapshot. Five tracked text inputs had no explicit checkout policy: `.gitattributes`, `.gitignore`, both license files, and `scripts/ci-linux-sandbox-canary.sh`. A clean checkout with `core.autocrlf=false` reproduced the Linux fingerprint; a clean checkout with `core.autocrlf=true` reproduced the Windows fingerprint. The existing local workspace had a mixed `.gitattributes` newline state and therefore a third fingerprint. The product bytes were semantically identical, but the checkout bytes were not.

## Fix

- Apply `* text=auto eol=lf` before the existing explicit extension rules so every detected text input has identical checkout bytes on Windows and Linux.
- Preserve `fixtures/compat/migration/** -text` as the later override because its manifest binds exact historical bytes, including the CRLF cache fixture.
- Add a Rust contract test that locks both policies, asserts the five previously drifting inputs are LF, and asserts the historical migration cache remains CRLF.
- Keep fingerprint v3's working-tree behavior unchanged; the repository checkout contract now makes those bytes portable instead of weakening edit detection.

## Remaining proof

The revised tree must produce one identical product fingerprint in clean `core.autocrlf=true` and `core.autocrlf=false` worktrees, pass the local candidate chain, and then pass a new hosted Windows MSVC plus Linux GNU candidate. No additional workflow dispatch is authorized by this resolution alone.
