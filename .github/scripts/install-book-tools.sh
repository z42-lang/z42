#!/usr/bin/env bash
set -euo pipefail

# Release assets are an acceleration; crates.io remains the source fallback.
book_tools_root="${BOOK_TOOLS_ROOT:-$HOME/.local}"
book_tools_stage="$(mktemp -d)"
trap 'rm -rf "$book_tools_stage"' EXIT
mkdir -p "$book_tools_root/bin"

install_book_tool() {
  local package="$1" version="$2" url="$3"
  local archive="$book_tools_stage/$package.tar.gz"
  if curl --fail --location --silent --show-error --retry 2 --connect-timeout 10 --max-time 30 \
      --output "$archive" "$url" && tar -tzf "$archive" >/dev/null 2>&1; then
    tar -xzf "$archive" -C "$book_tools_root/bin"
  else
    echo "Release download unavailable for $package; installing $version from crates.io"
    cargo install "$package" --version "$version" --locked --root "$book_tools_root"
  fi
}

install_book_tool mdbook 0.4.40 \
  https://github.com/rust-lang/mdBook/releases/download/v0.4.40/mdbook-v0.4.40-x86_64-unknown-linux-gnu.tar.gz
install_book_tool mdbook-mermaid 0.14.0 \
  https://github.com/badboy/mdbook-mermaid/releases/download/v0.14.0/mdbook-mermaid-v0.14.0-x86_64-unknown-linux-gnu.tar.gz
