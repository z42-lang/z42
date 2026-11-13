#!/usr/bin/env bash
set -eu
printf 'cmake %s\n' "$*" >> "$MOCK_LOG"
if [ "$1" = --build ]; then
  if [ "${MOCK_BUILD_EXIT:-0}" != 0 ]; then exit "$MOCK_BUILD_EXIT"; fi
  mkdir -p "$2/bin"
  printf '#!/usr/bin/env bash\necho "wasm-opt version %s"\n' "${MOCK_VERSION:-130}" > "$2/bin/wasm-opt"
  chmod +x "$2/bin/wasm-opt"
fi
