# Configuration, Secret, and File Reliability Plan

**Status:** Implemented and verified

## Goal

Make restart state trustworthy when writes are interrupted or local files are
damaged. Validate configuration before use, migrate fallback API keys out of
the workspace, atomically replace JSON snapshots with a valid backup, and
repair only a truncated final JSONL record.

## Implementation slices

1. Add failing real-filesystem tests for primary/backup recovery and JSONL tail
   repair.
2. Add deep configuration validation with field-specific errors and backup
   recovery.
3. Move non-keychain credentials to the user configuration directory and
   migrate the legacy workspace file transactionally.
4. Validate the thread index and recover its last valid snapshot.
5. Run the complete offline suite, strict TypeScript checking, and production
   build; update architecture documentation and the execution log.

## Recovery rules

- A JSON write creates the next file in the same directory, flushes it, and
  atomically replaces the destination.
- Before replacement, a valid current JSON snapshot becomes `.bak`.
- Invalid primary + valid backup restores the backup.
- Invalid primary + invalid/missing backup fails with the affected path; it
  never silently resets to defaults.
- A malformed final JSONL line is treated as an interrupted append and removed.
- Malformed JSONL before a later valid record is reported as corruption.
- Legacy credentials are deleted only after the new keychain/user-file write
  succeeds.

## Result

- Added atomic JSON replacement with file flush, same-directory temporary
  names, last-valid `.bak`, validated recovery, and fail-loud dual corruption.
- Added conservative JSONL tail repair and middle-corruption detection.
- Added deep configuration validation and thread-index validation.
- Moved fallback credentials to the OS user configuration directory with
  `MINIMAX_CODEX_HOME` override and transactional legacy migration.
- Rejected empty normalized API keys before any credential file is created.
- Verification: 60 tests passed, `npm run check` passed, and `npm run build`
  passed without real provider credentials.
