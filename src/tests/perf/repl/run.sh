#!/usr/bin/env bash
# src/tests/perf/repl/run.sh — REPL per-eval latency bench (perf-optimize-repl-eval).
#
# Drives the *real* `z42 repl` binary with a fixed input session and measures
# end-to-end wall-clock via hyperfine. The REPL's per-eval cost is dominated by
# `Script.Eval → PackageCompile → DepScan` re-decoding the whole stdlib+compiler
# world on every line (see docs/design/toolchain/repl.md Deferred). This bench is
# the tracking anchor for that optimization.
#
# Why drive the shipped binary (not an e2e scenario): Script.Eval pulls the whole
# compiler zpkg closure (z42c.* + z42.ir); a bare --emit-zbc scenario doesn't
# carry that dep closure, so only the packaged z42.interactive.zpkg entry runs it
# faithfully. Driving `z42 repl` measures exactly what a user feels.
#
# Usage:
#   src/tests/perf/repl/run.sh [z42-launcher] [runs] [warmup]
# Defaults: launcher=.z42/z42, runs=5, warmup=1.
#
# Records the 0/1/5-eval points so the per-eval slope (marginal cost) is separable
# from process+VM startup (the 0-eval point). Startup is ~0.13s; the slope is the
# number the optimization must move.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../../.." && pwd)"
z42="${1:-$root/.z42/z42}"
runs="${2:-5}"
warmup="${3:-1}"

if ! command -v hyperfine >/dev/null 2>&1; then
  echo "perf/repl: hyperfine not found (brew install hyperfine)" >&2
  exit 2
fi
if [ ! -x "$z42" ]; then
  echo "perf/repl: launcher not found/executable: $z42" >&2
  echo "  (run: ./scripts/install-z42.sh --version nightly, or xtask build sdk)" >&2
  exit 2
fi

echo "REPL eval latency — launcher: $z42"
echo "commit: $(git -C "$root" rev-parse --short HEAD 2>/dev/null || echo '?')  os: $(uname -sm)"
echo

hyperfine --warmup "$warmup" --runs "$runs" --shell=sh \
  --command-name "repl-0eval-startup"  "$z42 repl < $here/session_0eval.txt" \
  --command-name "repl-1eval"          "$z42 repl < $here/session_1eval.txt" \
  --command-name "repl-5eval"          "$z42 repl < $here/session_5eval.txt"
