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

# The runner only waits for sys.boot_completed; user storage (the app cache dir the
# embedded test copies its corpus into) and the package service can still be coming
# up. Wait until both answer before installing anything.
ready=0
for _ in $(seq 1 90); do
    if [ "$(adb shell getprop sys.user.0.ce_available 2>/dev/null | tr -d '\r')" = "true" ] \
        && adb shell pm path android >/dev/null 2>&1; then
        ready=1
        break
    fi
    sleep 2
done
if [ "$ready" -ne 1 ]; then
    echo "error: emulator not ready after 180s (sys.user.0.ce_available / package service)" >&2
    exit 1
fi

vm="$GITHUB_WORKSPACE/artifacts/build/runtime/release/z42vm"
libs="$GITHUB_WORKSPACE/artifacts/build/libraries/dist/release"

adb logcat -c || true
adb logcat > "$GITHUB_WORKSPACE/android-embed-logcat.txt" 2>&1 &
logcat_pid=$!

Z42_PORTABLE_VM="$vm" Z42_LIBS="$libs" "$vm" "$GITHUB_WORKSPACE/artifacts/xtask/xtask.zpkg" \
    -- test embedded --rid android-x64 --run
rc=$?

# Gradle's connectedAndroidTest reports BUILD SUCCESSFUL even when the test APK never
# installed (seen: `Requested internal only, but not enough space`), so a zero exit
# code alone proves nothing. No JUnit XML ⇒ no test ran ⇒ fail here, with the cause
# still visible above, instead of only at the reporter step.
results="$GITHUB_WORKSPACE/src/toolchain/workload/android/platform/z42vm/build/outputs/androidTest-results/connected"
if [ "$rc" -eq 0 ] && [ -z "$(find "$results" -name '*.xml' 2>/dev/null | head -1)" ]; then
    echo "error: connectedAndroidTest produced no JUnit XML under $results — no instrumented test ran" >&2
    rc=1
fi

sleep 3
kill "$logcat_pid" 2>/dev/null || true
adb pull /data/tombstones "$GITHUB_WORKSPACE/android-tombstones" 2>/dev/null || true
echo "===== android-embed-logcat.txt (tail 120, inline) ====="
tail -120 "$GITHUB_WORKSPACE/android-embed-logcat.txt" 2>/dev/null || echo "(no logcat captured)"
echo "===== end logcat tail ====="
exit "$rc"
