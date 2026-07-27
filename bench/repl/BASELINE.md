# REPL eval latency — baseline

Tracking anchor for `perf-optimize-repl-eval`. Run `bench/repl/run.sh` to reproduce.

## Baseline (before optimization)

- **Date**: 2026-07-26
- **SDK**: nightly (macos-arm64), installed via `scripts/install-z42.sh --version nightly`
- **Host**: Darwin arm64
- **Tool**: hyperfine 1.20.0

| session | wall-clock (mean ± σ) | per-eval marginal |
|---------|-----------------------|-------------------|
| 0 eval (startup only) | 0.13 s | — |
| 1 eval | 3.77 s | ~3.6 s |
| 3 eval | 10.65 s | ~3.5 s/eval |
| 5 eval | 18.23 s ± 0.07 | ~3.6 s/eval |

**Read**: per-eval cost is ~3.5 s and roughly constant; process+VM startup is
negligible (0.13 s) and session growth is minor over these sizes. The whole
per-eval cost is `Script.Eval → PackageCompile → DepScan` re-decoding the entire
stdlib+compiler zpkg world (~2–3.6 MB) through the interpreter, every line.

## Root cause (see analysis)

1. `DepScan.ScanDirs` re-reads + re-decodes every dep zpkg **twice** internally
   (world pre-open loop + main loop) — `src/compiler/z42c.pipeline/src/DepScan.z42`.
2. No cross-round cache: `PackageCompile.Compile` runs a full `DepScan` every
   eval; the stdlib+compiler world never changes within a session.
3. Expression→statement fallback compiles the whole thing twice for statement
   inputs — `src/toolchain/scripting/src/Script.z42`.

## Progress log

| date | change | 1-eval | 5-eval | 10-eval | marginal/eval | note |
|------|--------|--------|--------|---------|---------------|------|
| 2026-07-26 | (baseline) | 3.77 s | 18.23 s | — | ~3.6 s | nightly SDK |
| 2026-07-26 | ① DepScan single-decode | 3.42 s | 16.43 s | — | ~3.28 s | −10%; double-read ~10% of cost, not dominant |
| 2026-07-26 | ② cross-round scan cache | 3.39 s | **3.67 s** | 4.70 s | **~70–145 ms** | −80% @5-eval; marginal ~50× smaller |
| 2026-07-26 | ③⑤ stmt-skip + decl-only Vars | 3.4 s | 3.95 s | 4.15 s | **~72 ms flat** | ⑤ flattens O(n): only var-decl rounds grow the scan; expr rounds constant. 20-eval = 5.03 s |

> **② profiling proof**: DepScan.ScanDirs was ~3000 ms of the ~3200 ms/eval (98%);
> ImportedSymbolLoader ~50 ms, BuildPackageCus ~10 ms. Caching the static-libs
> DepScanResult once + `ExtendWithPackage` per carry-forward round eliminates the
> 98%. First eval still pays the one-time ~3.3 s static scan build.
>
> **Remaining**: (a) first-eval ~3.3 s one-time static scan — could persist across
> sessions (future ④). (b) marginal grows ~70→145 ms over 10 rounds because each
> needVars round appends a Vars{N} pkg to the cached scan (mild O(n); the deferred
> `repl-future-incremental-compilation`). Both are now one-time/small vs the old
> flat 3.5 s/eval wall.

> ① learning: removing DepScan's double read/decode gives ~10%. The remaining
> ~3.3 s/eval is the rest of the per-round full-world work (`TsigReconcile` rebuild
> + `ImportedSymbolLoader` over all exported symbols + typecheck against the whole
> import set) — targeted by ② (cache the dep world across rounds, since the
> stdlib+compiler world never changes within a session).
