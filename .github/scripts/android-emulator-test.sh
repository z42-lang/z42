#!/usr/bin/env bash
# CI helper for the `test-android-emu` job (ci.yml): runs R1–R7 + the embedded corpus
# on the emulator that reactivecircus/android-emulator-runner has already booted.
#
# Why a file instead of an inline `script: |` block: android-emulator-runner executes
# EVERY line of `script:` in its own `sh -c`, so shell variables never survive to the
# next line. The inline version (#340) expanded `"$vm"` to an empty string and died
# with `sh: 1: : Permission denied` (exit 127) — every nightly from 2026-08-30 on,
# without a single instrumented test having run. One line calling this file keeps all
# state in one shell.
#
# z42b-device-run PR-3: the emulator RUN 下沉 z42b's android driver — `test embedded
# --rid android-x64 --run` spawns `./gradlew :z42vm:connectedAndroidTest` (ONE emulator
# run = R1–R7 + embedded corpus); gradle writes the junit the reporter step reads.
# reactivecircus still supplies the emulator (design D2 asymmetry); z42b only triggers
# gradle. The step's working-directory is the android platform dir, but every path here
# is absolute and xtask's `_root()` uses `git rev-parse`, so cwd does not matter.
#
# diagnose-mobile-wasm-embed (#159): the embedded corpus can crash the app process
# natively ("Instrumentation run failed due to Process crashed") — the cause lives in
# logcat/tombstones, not the JUnit XML, and a post-crash `adb logcat -d` returns
# nothing. So logcat is captured CONTINUOUSLY in the background and its tail is echoed
# inline into the job log.
set -u

vm="$GITHUB_WORKSPACE/artifacts/build/runtime/release/z42vm"
libs="$GITHUB_WORKSPACE/artifacts/build/libraries/dist/release"

adb logcat -c || true
adb logcat > "$GITHUB_WORKSPACE/android-embed-logcat.txt" 2>&1 &
logcat_pid=$!

Z42_PORTABLE_VM="$vm" Z42_LIBS="$libs" "$vm" "$GITHUB_WORKSPACE/artifacts/xtask/xtask.zpkg" \
    -- test embedded --rid android-x64 --run
rc=$?

sleep 3
kill "$logcat_pid" 2>/dev/null || true
adb pull /data/tombstones "$GITHUB_WORKSPACE/android-tombstones" 2>/dev/null || true
echo "===== android-embed-logcat.txt (tail 120, inline) ====="
tail -120 "$GITHUB_WORKSPACE/android-embed-logcat.txt" 2>/dev/null || echo "(no logcat captured)"
echo "===== end logcat tail ====="
exit "$rc"
