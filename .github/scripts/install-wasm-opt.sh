#!/usr/bin/env bash
set -euo pipefail
# Same Binaryen version wasm-pack 0.15.1 requested; no optimization is disabled.
wasm_opt_root="${WASM_OPT_ROOT:?set WASM_OPT_ROOT}"
if [[ -x "$wasm_opt_root/bin/wasm-opt" ]] && "$wasm_opt_root/bin/wasm-opt" --version | grep -Eq 'version 130([ .(]|$)'; then
  exit 0
fi
wasm_opt_stage="$(mktemp -d)"
trap 'rm -rf "$wasm_opt_stage"' EXIT
git init --quiet "$wasm_opt_stage/src"
git -C "$wasm_opt_stage/src" remote add origin https://github.com/WebAssembly/binaryen.git
git -C "$wasm_opt_stage/src" fetch --depth 1 origin 5d704ad52bc77a258e8fa3f9d34fcc5e8799c1c3
git -C "$wasm_opt_stage/src" checkout --detach FETCH_HEAD
cmake -S "$wasm_opt_stage/src" -B "$wasm_opt_stage/build" \
  -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DBUILD_TESTS=OFF \
  -DBYN_ENABLE_ASSERTIONS=OFF -DENABLE_WERROR=OFF
cmake --build "$wasm_opt_stage/build" --target wasm-opt --parallel 2
mkdir -p "$wasm_opt_root/bin"
install -m 755 "$wasm_opt_stage/build/bin/wasm-opt" "$wasm_opt_root/bin/wasm-opt"
"$wasm_opt_root/bin/wasm-opt" --version | grep -Eq 'version 130([ .(]|$)'
