#!/usr/bin/env bash
# Exercise the verdict rules with the committed inputs from the repository root.
# Keep the 0.15 column in sync with the measurement threshold in bench-pr.yml.
set -uo pipefail
T="src/benches/testdata"
fail=0
xt() {
  xtask "$@" >/dev/null 2>&1
  echo $?
}
expect() { # <label> <want> <got>
  if [ "$3" = "$2" ]; then
    echo "✅ $1 → exit $3"
  else
    echo "::error::verdict self-check: $1 exited $3, expected $2"
    fail=1
  fi
}

# 1) _abVerdict + _abResampleVerdict + pause-gate pure-function unit check (17 cases).
expect "ab-selftest" 0 "$(xt bench --ab-selftest)"

# 2) _benchDiff — gate is `delta > thr AND cur.ci_lower > base.ci_upper`.
for c in overlap:0:0 regress:1:1 improve:0:0 separated-under-threshold:1:0; do
  name="${c%%:*}"; rest="${c#*:}"; w15="${rest%%:*}"; w25="${rest##*:}"
  expect "diff/$name@0.15" "$w15" \
    "$(xt bench --diff --current "$T/current-$name.json" --baseline "$T/baseline.json" --threshold-time 0.15)"
  expect "diff/$name@0.25" "$w25" \
    "$(xt bench --diff --current "$T/current-$name.json" --baseline "$T/baseline.json" --threshold-time 0.25)"
done

# 3) _microDiff single round — gate is R_lower > 1+thr, per name+metric+mode key.
#    fallback-* drive the no-SEM interval path (base carries mean_ns=-1);
#    unmatched holds three 3x-blown-up entries whose name or mode key has
#    no base counterpart — they must be SKIPPED, so a loosened match
#    turns this case red.
for c in regress:1:1 overlap:0:0 improve:0:0 separated-under-threshold:1:0 \
         fallback-regress:1:1 fallback-overlap:0:0 unmatched:0:0; do
  name="${c%%:*}"; rest="${c#*:}"; w15="${rest%%:*}"; w25="${rest##*:}"
  expect "micro/$name@0.15" "$w15" \
    "$(xt bench --micro-diff --current "$T/micro-$name.json" --baseline "$T/micro-base.json" --threshold-time 0.15)"
  expect "micro/$name@0.25" "$w25" \
    "$(xt bench --micro-diff --current "$T/micro-$name.json" --baseline "$T/micro-base.json" --threshold-time 0.25)"
done

# 4) _microDiff RESAMPLED — three rounds per side,
#    verdict from the BETWEEN-ROUND ratio dispersion. Each case guards one thing:
#      rs-drift   round 1 +60%, rounds 2-3 +5%/+10%. MEAN ratio (1.25) is above
#                 1+0.15 on purpose ⇒ green only if the interval width is computed.
#      rs-real    +30% in every round ⇒ still red (resampling did not blind the gate).
#      rs-mid     +20%-ish, the thr-sensitive one: red at 0.15, quiet at 0.25.
#      rs-missing regressing entry absent from round 3 ⇒ EXCLUDED (warning) ⇒ green.
#    Base rounds carry DIFFERENT values per round ⇒ pairing pr round i with the
#    wrong base round flips rs-mid@0.15.
RSB="$T/micro-rs-base-r1.json,$T/micro-rs-base-r2.json,$T/micro-rs-base-r3.json"
for c in drift:0:0 real:1:1 mid:1:0; do
  name="${c%%:*}"; rest="${c#*:}"; w15="${rest%%:*}"; w25="${rest##*:}"
  cur="$T/micro-rs-$name-r1.json,$T/micro-rs-$name-r2.json,$T/micro-rs-$name-r3.json"
  expect "micro-resample/$name@0.15" "$w15" \
    "$(xt bench --micro-diff --current "$cur" --baseline "$RSB" --threshold-time 0.15)"
  expect "micro-resample/$name@0.25" "$w25" \
    "$(xt bench --micro-diff --current "$cur" --baseline "$RSB" --threshold-time 0.25)"
done
MISS="$T/micro-rs-real-r1.json,$T/micro-rs-real-r2.json,$T/micro-rs-missing-r3.json"
expect "micro-resample/missing@0.15" 0 \
  "$(xt bench --micro-diff --current "$MISS" --baseline "$RSB" --threshold-time 0.15)"
expect "micro-resample/missing@0.25" 0 \
  "$(xt bench --micro-diff --current "$MISS" --baseline "$RSB" --threshold-time 0.25)"
# 轮数不匹配必须是**用法错误**（exit 2），不能被当成「没有回归」放行。
expect "micro-resample/round-count-mismatch" 2 \
  "$(xt bench --micro-diff --current "$MISS" --baseline "$T/micro-rs-base-r1.json" --threshold-time 0.15)"

[ "$fail" -eq 0 ] || { echo "❌ 判红规则与 fixture 不符 —— 改了阈值或区间条件?"; exit 1; }
echo "✅ verdict logic matches all fixtures (17 pure-function cases + 15 fixtures x 2 thresholds + 1 usage-error case)"
