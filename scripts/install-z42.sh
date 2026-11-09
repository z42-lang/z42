#!/bin/sh
# z42 installer — macOS / Linux.
#
#   curl -fsSL https://z42-lang.github.io/z42/install-z42.sh | sh
#   curl -fsSL https://z42-lang.github.io/z42/install-z42.sh | sh -s -- --version 0.6.0
#   ./scripts/install-z42.sh                       # in a z42 checkout
#
# One file, two default sets, chosen by where the script itself lives (never by cwd):
#   repository — invoked by its path inside a checkout (<repo>/scripts/install-z42.sh, next to
#                xtask.z42.toml, with <repo>/src/compiler/): version pinned in scripts/versions.toml
#                ([toolchain.z42].launcher), installs into <repo>/.z42, never touches PATH.
#   user       — anything else (downloaded copy, `curl | sh`, a symlink elsewhere): $Z42_VERSION or
#                nightly, $Z42_HOME or ~/.z42, adds z42 to PATH in your shell profile.
# Explicit --version / --dest override either set. Re-run to update. Windows: see install-z42.ps1.
#
# POSIX sh on purpose (works when piped into `sh`); needs curl or wget, tar,
# and sha256sum or shasum. No python.
set -eu

REPO_SLUG="z42-lang/z42"
DOCS_URL="https://z42-lang.github.io/z42/learn/"

usage() {
  cat <<EOF
z42 installer

usage: install-z42.sh [options]

options:
  --version <v>        version to install: nightly or x.y.z
  --dest <dir>         install directory
  --no-modify-path     do not add z42 to PATH in your shell profile
  --archive <file>     install from a local SDK archive instead of downloading
  --force              reinstall even if this version is already installed
  --dry-run            show what would happen, change nothing
  -h, --help           show this help

defaults:
  in a z42 checkout    version pinned in scripts/versions.toml, <repo>/.z42, PATH untouched
  otherwise            \$Z42_VERSION or nightly, \$Z42_HOME or ~/.z42, PATH set up
EOF
}

say()  { printf 'z42-install: %s\n' "$*"; }
err()  { printf 'z42-install: error: %s\n' "$*" >&2; exit 1; }

# ── defaults: repository or user ──────────────────────────────────────────────
# $0 is the invoked path (piped: the shell's own name). A file symlink is not followed.
INSTALL_MODE=user
REPO_ROOT=""
case "$0" in
  */install-z42.sh|install-z42.sh)
    if [ -f "$0" ]; then
      SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
      CANDIDATE=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)
      if [ "$SCRIPT_DIR" = "$CANDIDATE/scripts" ] &&
         [ -f "$SCRIPT_DIR/xtask.z42.toml" ] && [ -d "$CANDIDATE/src/compiler" ]; then
        INSTALL_MODE=repository
        REPO_ROOT=$CANDIDATE
      fi
    fi ;;
esac

if [ "$INSTALL_MODE" = repository ]; then
  PINS="$REPO_ROOT/scripts/versions.toml"
  [ -f "$PINS" ] || err "repository pin file is missing: $PINS"
  # Reads `launcher = "<v>"` under [toolchain.z42]; `xtask package sdk --verify` checks this
  # reading against the real TOML parser on the checked-in file.
  VERSION=$(awk '
    /^[[:space:]]*\[toolchain\.z42\][[:space:]]*(#.*)?$/ { active = 1; next }
    /^[[:space:]]*\[/ { active = 0 }
    active && /^[[:space:]]*launcher[[:space:]]*=[[:space:]]*"[^"]+"[[:space:]]*(#.*)?$/ {
      if (match($0, /"[^"]+"/)) { print substr($0, RSTART + 1, RLENGTH - 2); exit }
    }
  ' "$PINS")
  [ -n "$VERSION" ] || err "repository versions.toml has no valid toolchain.z42.launcher"
  DEST="$REPO_ROOT/.z42"
  MODIFY_PATH=0
else
  VERSION="${Z42_VERSION:-nightly}"
  DEST="${Z42_HOME:-${HOME:+$HOME/.z42}}"
  MODIFY_PATH=1
fi
ARCHIVE=""
FORCE=0
DRY_RUN=0

while [ $# -gt 0 ]; do
  case "$1" in
    --version)        [ $# -ge 2 ] || err "--version needs a value"; VERSION="$2"; shift 2 ;;
    --dest)           [ $# -ge 2 ] || err "--dest needs a value"; DEST="$2"; shift 2 ;;
    --no-modify-path) MODIFY_PATH=0; shift ;;
    --archive)        [ $# -ge 2 ] || err "--archive needs a value"; ARCHIVE="$2"; shift 2 ;;
    --force)          FORCE=1; shift ;;
    --dry-run)        DRY_RUN=1; shift ;;
    -h|--help)        usage; exit 0 ;;
    *)                err "unknown option: $1 (see --help)" ;;
  esac
done

[ -n "$VERSION" ] || err "version must not be empty"
[ -n "$DEST" ] || err "cannot determine install directory (HOME unset); pass --dest"
if [ "$MODIFY_PATH" -eq 1 ]; then PATH_POLICY=yes; else PATH_POLICY=no; fi
if [ "$INSTALL_MODE" = repository ]; then say "mode: repository ($REPO_ROOT)"; else say "mode: user"; fi
say "version: $VERSION"
say "install: $DEST"
say "modify PATH: $PATH_POLICY"

# ── platform ──────────────────────────────────────────────────────────────────
os="$(uname -s)"; arch="$(uname -m)"
case "$os/$arch" in
  Darwin/arm64)              RID="macos-arm64" ;;
  Linux/x86_64|Linux/amd64)  RID="linux-x64" ;;
  Linux/aarch64|Linux/arm64) RID="linux-arm64" ;;
  Darwin/x86_64) err "Intel Macs are not supported yet (supported: macOS arm64, Linux x64/arm64, Windows x64)" ;;
  *)             err "unsupported platform $os/$arch (supported: macOS arm64, Linux x64/arm64, Windows x64)" ;;
esac

if [ "$VERSION" = "nightly" ]; then TAG="nightly"; else TAG="v$VERSION"; fi
ASSET="z42-sdk-$VERSION-$RID.tar.gz"
BASE_URL="https://github.com/$REPO_SLUG/releases/download/$TAG"

if [ "$DRY_RUN" -eq 1 ]; then
  say "dry run — nothing will be changed"
  say "  platform: $RID"
  if [ -n "$ARCHIVE" ]; then say "  archive: $ARCHIVE"; else say "  download: $BASE_URL/$ASSET"; fi
  exit 0
fi

# ── tools ─────────────────────────────────────────────────────────────────────
download() {  # url out
  if command -v curl >/dev/null 2>&1; then curl -fsSL --retry 3 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then wget -q -O "$2" "$1"
  else err "curl or wget is required"; fi
}
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1
  else err "sha256sum or shasum is required"; fi
}
command -v tar >/dev/null 2>&1 || err "tar is required"

TMP="$(mktemp -d 2>/dev/null || mktemp -d -t z42-install)"
trap 'rm -rf "$TMP"' EXIT INT TERM

# ── fetch + verify ────────────────────────────────────────────────────────────
SHA=""
if [ -n "$ARCHIVE" ]; then
  [ -f "$ARCHIVE" ] || err "archive not found: $ARCHIVE"
  PKG="$ARCHIVE"
  say "installing from $ARCHIVE"
else
  download "$BASE_URL/SHA256SUMS" "$TMP/SHA256SUMS" || err "download failed: $BASE_URL/SHA256SUMS"
  SHA="$(awk -v a="$ASSET" '$2 == a || $2 == "*" a { print $1; exit }' "$TMP/SHA256SUMS")"
  [ -n "$SHA" ] || err "no checksum for $ASSET in SHA256SUMS"
  # The same archive is already installed → nothing to do (re-running is how you update).
  if [ "$FORCE" -eq 0 ] && [ -x "$DEST/z42" ] && grep -qs "^sha256 = \"$SHA\"" "$DEST/install.toml"; then
    say "z42 $VERSION is already up to date in $DEST"
    exit 0
  fi
  PKG="$TMP/$ASSET"
  say "downloading z42 $VERSION for $RID"
  download "$BASE_URL/$ASSET" "$PKG" || err "download failed: $BASE_URL/$ASSET"
  [ "$(sha256_of "$PKG")" = "$SHA" ] || err "checksum mismatch for $ASSET"
fi

# ── install ───────────────────────────────────────────────────────────────────
# Extract to a staging dir next to DEST, then replace exactly the top-level entries the archive
# carries; everything else in DEST (installed workloads under runtimes/, cache/, install.toml)
# is kept. The SDK never ships those state entries — `xtask package sdk --verify` checks it.
mkdir -p "$DEST"
STAGE="$DEST/.install-staging"
rm -rf "$STAGE"; mkdir -p "$STAGE"
tar -xzf "$PKG" -C "$STAGE" || err "failed to extract $PKG"
[ -f "$STAGE/z42" ] && [ -d "$STAGE/bin" ] || err "archive does not look like a z42 SDK (no z42 / bin/)"
for src in "$STAGE"/*; do
  entry=${src##*/}
  rm -rf "${DEST:?}/$entry"
  mv "$src" "$DEST/$entry"
done
rm -rf "$STAGE"
chmod +x "$DEST/z42" "$DEST"/bin/* 2>/dev/null || true
printf 'version = "%s"\nrid = "%s"\nsha256 = "%s"\n' "$VERSION" "$RID" "$SHA" > "$DEST/install.toml"

INSTALLED="$("$DEST/z42" --version 2>/dev/null)" || INSTALLED=""
say "installed ${INSTALLED:-z42} to $DEST"

# ── PATH ──────────────────────────────────────────────────────────────────────
on_path() { case ":${PATH:-}:" in *":$1:"*) return 0 ;; *) return 1 ;; esac; }
LINE="export PATH=\"$DEST:$DEST/bin:\$PATH\""
if on_path "$DEST"; then
  :
elif [ "$MODIFY_PATH" -eq 1 ]; then
  case "$(basename "${SHELL:-sh}")" in
    zsh)  PROFILE="$HOME/.zshrc" ;;
    bash) if [ "$os" = "Darwin" ]; then PROFILE="$HOME/.bash_profile"; else PROFILE="$HOME/.bashrc"; fi ;;
    fish) PROFILE="$HOME/.config/fish/conf.d/z42.fish"; LINE="fish_add_path \"$DEST\" \"$DEST/bin\"" ;;
    *)    PROFILE="$HOME/.profile" ;;
  esac
  mkdir -p "$(dirname "$PROFILE")"
  if ! grep -qs '# added by the z42 installer' "$PROFILE"; then
    printf '\n# added by the z42 installer\n%s\n' "$LINE" >> "$PROFILE"
    say "added z42 to PATH in $PROFILE"
  fi
  say "restart your terminal (or run: $LINE), then:"
else
  say "add z42 to PATH:  $LINE"
  say "then:"
fi
cat <<EOF

    z42 --version
    z42 new hello && cd hello && z42 run

  learn more: $DOCS_URL
EOF
