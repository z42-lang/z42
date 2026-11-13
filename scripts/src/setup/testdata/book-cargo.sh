#!/usr/bin/env bash
printf '%s\n' "$*" >> "$MOCK_LOG"
exit "${MOCK_CARGO_EXIT:-0}"
