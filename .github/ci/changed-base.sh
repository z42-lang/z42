#!/usr/bin/env bash
# Print a verified test-diff base; empty output selects the full test path.
# Publishing uses the separate cumulative nightly diff in main-supersede.sh.
set -euo pipefail

case "${GITHUB_EVENT_NAME:-}" in
  pull_request) base="${XTASK_PR_BASE:-}" ;;
  push) base="${XTASK_PUSH_BEFORE:-}" ;;
  *) exit 0 ;;
esac

if [ -z "$base" ] || [ "$base" = "0000000000000000000000000000000000000000" ]; then
  echo "::notice::No test-diff base; running the full test path" >&2
  exit 0
fi

if git fetch --no-tags --depth=1 origin "$base" >&2 \
   && git rev-parse --verify --quiet "$base^{commit}" >/dev/null 2>&1; then
  printf '%s\n' "$base"
else
  echo "::notice::Cannot verify test-diff base; running the full test path" >&2
fi
