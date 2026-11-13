#!/usr/bin/env bash
set -eu
tool="${0##*/}"
printf '%s %s\n' "$tool" "$*" >> "$BOOTSTRAP_FIXTURE_ROOT/calls.log"
if [ "$tool" = z42vm ]; then exit 67; fi
if [ "$tool" = z42c ] && [ "${FAIL_XTASK_COMPILER:-0}" = 1 ]; then exit 23; fi
if [ "$tool" = z42c ]; then
  mkdir -p "$BOOTSTRAP_FIXTURE_ROOT/artifacts/xtask"
  printf fixture > "$BOOTSTRAP_FIXTURE_ROOT/artifacts/xtask/xtask.zpkg"
else
  mkdir -p "$BOOTSTRAP_FIXTURE_ROOT/artifacts/build/compiler/z42c.driver/release/dist" "$BOOTSTRAP_FIXTURE_ROOT/artifacts/intermediate/libraries/flat/release"
  printf fixture > "$BOOTSTRAP_FIXTURE_ROOT/artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg"
  printf fixture > "$BOOTSTRAP_FIXTURE_ROOT/artifacts/intermediate/libraries/flat/release/z42.core.zpkg"
fi
