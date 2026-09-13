#!/usr/bin/env bash
# test.sh — Run Z42VMInstrumentedTest (R1–R7) inside the Pixel 6 API 37
# emulator (AVD z42_pixel6_api37, created by install-android-toolchain-local.sh).
#
# Prereqs:
#   1. ./scripts/install-android-toolchain-local.sh   (one-shot; SDK + NDK + AVD + Gradle)
#   2. ./build.sh                                     (produces AAR + test fixtures)
#
# This script:
#   - Exports ANDROID_HOME / ANDROID_NDK_HOME / GRADLE_USER_HOME / JAVA_HOME
#   - Reuses an already-running emulator if `adb devices` sees one;
#     otherwise launches `@z42_pixel6_api37` headless in the background
#     and traps EXIT to `adb emu kill` it on script exit.
#   - Polls `sys.boot_completed` until the device is ready (~60s typical).
#   - Runs `./gradlew :z42vm:connectedAndroidTest`.
#
# Spec: docs/spec/archive/2026-05-12-add-android-tests/

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../../../.." && pwd)"
TOOLS_DIR="$ROOT/artifacts/tools"

# ── Env setup. ───────────────────────────────────────────────────────────

export ANDROID_HOME="$TOOLS_DIR/android-sdk"
export ANDROID_NDK_HOME="$TOOLS_DIR/android-ndk"
export GRADLE_USER_HOME="$TOOLS_DIR/gradle-user-home"

if [[ -z "${JAVA_HOME:-}" ]] && [[ -x "/usr/libexec/java_home" ]]; then
    export JAVA_HOME="$(/usr/libexec/java_home -v 17+ 2>/dev/null || /usr/libexec/java_home 2>/dev/null)"
fi

# Validate toolchain.
for path in \
    "$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager" \
    "$ANDROID_HOME/emulator/emulator" \
    "$ANDROID_HOME/platform-tools/adb"; do
    [[ -x "$path" ]] || {
        echo "error: $path missing. Run ./scripts/install-android-toolchain-local.sh first." >&2
        exit 1
    }
done

ADB="$ANDROID_HOME/platform-tools/adb"
EMULATOR="$ANDROID_HOME/emulator/emulator"
AVDMANAGER="$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager"
AVD_NAME="z42_pixel6_api37"

# Confirm AVD exists.
if ! "$AVDMANAGER" list avd 2>/dev/null | awk -v want="$AVD_NAME" '/Name:/ && $2==want {found=1} END {exit !found}'; then
    echo "error: AVD '$AVD_NAME' not found. Run ./scripts/install-android-toolchain-local.sh." >&2
    exit 1
fi

# ── Emulator: reuse running, else launch headless. ───────────────────────

LAUNCHED=false
if "$ADB" devices | grep -q '^emulator-[0-9]*'; then
    echo "reusing running emulator"
else
    echo "launching emulator $AVD_NAME (headless)…"
    LOG="$TOOLS_DIR/emulator.log"
    "$EMULATOR" "@$AVD_NAME" \
        -no-window -no-audio -no-boot-anim \
        -gpu swiftshader_indirect \
        > "$LOG" 2>&1 &
    EMU_PID=$!
    LAUNCHED=true
    trap '
        echo "shutting down emulator…"
        "$ADB" emu kill 2>/dev/null || true
        kill $EMU_PID 2>/dev/null || true
    ' EXIT
fi

# ── Wait for boot. ───────────────────────────────────────────────────────

echo "adb wait-for-device …"
"$ADB" wait-for-device

echo "waiting for sys.boot_completed (typical ~60s)…"
for i in $(seq 1 120); do
    BOOT_FLAG="$("$ADB" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')"
    if [[ "$BOOT_FLAG" = "1" ]]; then
        echo "  ✓ boot_completed (after ${i}×2s)"
        break
    fi
    sleep 2
    if (( i == 120 )); then
        echo "error: emulator did not boot within 240s" >&2
        exit 1
    fi
done

# Extra settle time so Activity / Service services are alive.
sleep 3

# ── Run instrumented tests. ──────────────────────────────────────────────

cd "$HERE"
echo ""
echo "./gradlew :z42vm:connectedAndroidTest"
echo ""
./gradlew :z42vm:connectedAndroidTest

echo ""
echo "✅ Z42VMInstrumentedTest passed"
