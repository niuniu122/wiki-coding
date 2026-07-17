#!/usr/bin/env bash
set -euo pipefail

: "${RUNNER_TEMP:?GitHub Actions must provide RUNNER_TEMP}"

real_cargo="$(command -v cargo)"
wrapper_dir="$RUNNER_TEMP/wiki-coding-cargo-wrapper"
mkdir -p "$wrapper_dir"
printf '#!/bin/sh\nexec "%s" "$@" 2>&1\n' "$real_cargo" > "$wrapper_dir/cargo"
chmod 0755 "$wrapper_dir/cargo"

PATH="$wrapper_dir:$PATH" cargo test -p minimax-tools --test sandbox_adversarial --locked
