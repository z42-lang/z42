#!/usr/bin/env bash
# compare-modes.sh — 对比同一场景在 interp / jit 下的端到端耗时。
#
# 背景：xtask bench 只跑「默认模式」单跑，从不 sweep interp vs jit（见
# scripts/xtask_bench.z42：run 行不带 --mode）。JIT 1.5×/6× 这类数字此前只
# 以源码注释形式手记于 bench/scenarios/04，不可复现。本脚本补上「同一 zbc、
# 两种模式、hyperfine 测量」的对比能力,产出 bench/results/mode-comparison.json,
# 作为 perf-vm-iteration 的回归基线。
#
# 用法:
#   bench/scripts/compare-modes.sh [z42vm 路径] [runs] [warmup]
# 默认 z42vm = artifacts/build/runtime/release/z42vm（需先 cargo build --release）。
# 依赖: hyperfine, python3, 一套已构建的 stdlib（.z42/libs）与 z42c driver。
#
# 注意: 03_startup 场景是「VM+stdlib 加载」基线；把它从其它场景耗时里减去,
# 才是真实「计算耗时」。JIT 冷编译成本(~8ms)也计入,故极短场景 JIT 可能更慢。
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VM="${1:-$ROOT/artifacts/build/runtime/release/z42vm}"
RUNS="${2:-8}"
WARMUP="${3:-2}"
DRIVER="$ROOT/.z42/programs/z42c/z42c.driver.zpkg"
LIBS="$ROOT/.z42/libs"
OUT="$ROOT/bench/results/mode-comparison.json"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

export Z42_LIBS="$LIBS"

echo "z42vm : $VM"
echo "libs  : $LIBS"
echo

# 编译全部场景到 zbc（用 SDK z42vm 跑 driver;编译模式不影响产物）。
for f in "$ROOT"/bench/scenarios/*.z42; do
  name="$(basename "$f" .z42)"
  "$ROOT/.z42/bin/z42vm" "$DRIVER" --mode jit -- --emit-zbc "$f" "$WORK/$name.zbc" >/dev/null 2>&1 \
    && echo "compiled $name" || { echo "FAILED to compile $name" >&2; exit 1; }
done
echo

printf "%-26s %11s %11s %8s\n" "scenario" "interp(ms)" "jit(ms)" "jit x"
printf "%-26s %11s %11s %8s\n" "--------" "----------" "-------" "-----"

echo "{" > "$OUT"
echo "  \"tool\": \"compare-modes.sh\", \"runs\": $RUNS, \"scenarios\": [" >> "$OUT"
first=1
for f in "$ROOT"/bench/scenarios/*.z42; do
  name="$(basename "$f" .z42)"
  ns="$(grep -m1 '^namespace' "$f" | sed 's/^namespace //; s/;.*//')"
  zbc="$WORK/$name.zbc"
  hyperfine -N --warmup "$WARMUP" --runs "$RUNS" --export-json "$WORK/$name.json" \
    "$VM $zbc $ns.Main --mode interp" \
    "$VM $zbc $ns.Main --mode jit" >/dev/null 2>&1
  im="$(python3 -c "import json;print(json.load(open('$WORK/$name.json'))['results'][0]['mean']*1000)")"
  jm="$(python3 -c "import json;print(json.load(open('$WORK/$name.json'))['results'][1]['mean']*1000)")"
  sp="$(python3 -c "print(f'{$im/$jm:.2f}')")"
  printf "%-26s %11.1f %11.1f %7sx\n" "$name" "$im" "$jm" "$sp"
  [ $first -eq 0 ] && echo "," >> "$OUT"; first=0
  printf '    {"scenario": "%s", "ns": "%s", "interp_ms": %.2f, "jit_ms": %.2f, "jit_speedup": %s}' \
    "$name" "$ns" "$im" "$jm" "$sp" >> "$OUT"
done
echo "" >> "$OUT"
echo "  ]" >> "$OUT"
echo "}" >> "$OUT"
echo
echo "wrote $OUT"
