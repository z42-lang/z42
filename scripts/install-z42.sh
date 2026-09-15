#!/usr/bin/env bash
# install-z42.sh — z42 bootstrap / system install (2026-06-13).
# Copyright (c) codesigner-ui. MIT licence.
#
# Downloads the prebuilt z42 launcher package from GitHub Releases and installs
# it. Two install modes:
#
#   Portable (default): extract the raw package to a single flat directory.
#   Managed (--system): structured bin/launcher/runtimes layout + PATH hint.
#
# For usage, run:  install-z42.sh --help
set -euo pipefail

# ── stream setup ─────────────────────────────────────────────────────────────
# Use stream 3 for user-facing output so say() / say_verbose() don't pollute
# return values from functions that write to stdout. Pattern from dotnet-install.
exec 3>&1

# ── color setup ──────────────────────────────────────────────────────────────
bold="" normal="" red="" green="" yellow="" cyan="" gray=""
if [ -t 1 ] && command -v tput >/dev/null 2>&1; then
  ncolors=$(tput colors 2>/dev/null || echo 0)
  if [ -n "$ncolors" ] && [ "$ncolors" -ge 8 ]; then
    bold="$(tput bold   || echo)"
    normal="$(tput sgr0  || echo)"
    red="$(tput setaf 1  || echo)"
    green="$(tput setaf 2 || echo)"
    yellow="$(tput setaf 3 || echo)"
    cyan="$(tput setaf 6  || echo)"
    gray="$(tput setaf 8  || echo)"  # dim / dark gray on 256-color terminals
  fi
fi

# ── output helpers ────────────────────────────────────────────────────────────
say()         { printf "%b\n" "${cyan}install-z42:${normal} $1" >&3; }
say_ok()      { printf "%b\n" "${green}install-z42:${normal} $1" >&3; }
say_warn()    { printf "%b\n" "${yellow}install-z42: Warning:${normal} $1" >&3; }
say_err()     { printf "%b\n" "${red}install-z42: Error:${normal} $1" >&2; }
say_verbose() { if [ "$VERBOSE" = "1" ]; then printf "%b\n" "${gray}  [verbose] $1${normal}" >&3; fi; }

# ── help ──────────────────────────────────────────────────────────────────────
_help() {
  cat >&3 <<EOF
${bold}install-z42.sh${normal} — download and install the z42 language runtime

${bold}USAGE${normal}
  install-z42.sh [OPTIONS]

${bold}OPTIONS${normal}
  ${bold}--version${normal} <version>   Version to install.  Default: read from
                         versions.toml [toolchain.z42].launcher.
                         Pass "nightly" for the latest nightly build,
                         or a semver string like "0.3.0" for a stable release.

  ${bold}--dest${normal} <dir>          Directory to install into.
                         Default (portable): <repo>/.z42
                         Default (--system): \$Z42_HOME  or  ~/.z42

  ${bold}--system${normal}             Install into \$Z42_HOME (or ~/.z42) instead of <repo>/.z42.
                         Re-run this script to update.

  ${bold}--dry-run${normal}            Print what would be downloaded and installed,
                         but do not actually download or write any files.

  ${bold}--no-path${normal}            Suppress the PATH suggestion printed after a
                         --system install.  Useful for scripted/CI invocations.

  ${bold}--verbose${normal}            Enable verbose diagnostic output.

  ${bold}--help${normal}, ${bold}-h${normal}          Show this help message.

${bold}INSTALL MODES${normal}
  ${bold}Portable${normal} (default)
    Extracts the package to a single flat directory.
    Entry: <dest>/z42  — add <dest> to PATH or invoke directly.
    Ideal for project-local bootstrap (.z42/ beside the repo).

  ${bold}Managed${normal} (--system)
    Installs into a Z42_HOME-style root that the launcher understands.
    Multiple runtime versions can coexist under <dest>/runtimes/.
    Add <dest>/bin to your shell's PATH; \`z42 run app.zpkg\` then works
    from any directory.

${bold}EXAMPLES${normal}
  # Bootstrap the project-local .z42 (most common dev usage):
  ./scripts/install-z42.sh

  # Bootstrap a specific version:
  ./scripts/install-z42.sh --version 0.3.0

  # Global managed install, then add to PATH:
  ./scripts/install-z42.sh --system
  export PATH="\$HOME/.z42/bin:\$PATH"

  # Managed install to a custom directory:
  ./scripts/install-z42.sh --dest /opt/z42 --system

  # Portable install to a CI cache directory:
  ./scripts/install-z42.sh --dest /tmp/z42-ci

  # Preview what nightly would install without writing anything:
  ./scripts/install-z42.sh --version nightly --dry-run

${bold}NOTES${normal}
  Version source: versions.toml [toolchain.z42].launcher  (override with --version).
  SHA256:         verified from release-index.json or SHA256SUMS.
  Staleness:      nightly re-downloads only when the published timestamp changes.
  Windows:        use install-z42.bat (supports the same flags).
  macOS Finder:   double-click install-z42.command (runs this script without args).

EOF
}

# ── default option values ─────────────────────────────────────────────────────
SYSTEM_INSTALL=0
USER_DEST=""
VERSION_OVERRIDE=""
DRY_RUN=0
NO_PATH=0
VERBOSE=0

# ── parse flags ───────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --system)             SYSTEM_INSTALL=1; shift ;;
    --dest)               USER_DEST="${2:-}"; shift 2 ;;
    --dest=*)             USER_DEST="${1#--dest=}"; shift ;;
    --version)            VERSION_OVERRIDE="${2:-}"; shift 2 ;;
    --version=*)          VERSION_OVERRIDE="${1#--version=}"; shift ;;
    --dry-run)            DRY_RUN=1; shift ;;
    --no-path)            NO_PATH=1; shift ;;
    --verbose)            VERBOSE=1; shift ;;
    --help|-h)            _help; exit 0 ;;
    *) say_err "unknown flag: $1  (try --help)"; exit 1 ;;
  esac
done

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SLUG="codesigner-ui/z42"

if [ $SYSTEM_INSTALL -eq 1 ]; then
  DEST="${USER_DEST:-${Z42_HOME:-$HOME/.z42}}"
else
  DEST="${USER_DEST:-$REPO/.z42}"
fi

STAMP="$DEST/.bootstrap-stamp"

# ── 1. version ────────────────────────────────────────────────────────────────
if [ -n "$VERSION_OVERRIDE" ]; then
  VER="$VERSION_OVERRIDE"
  say_verbose "version: $VER (--version override)"
else
  VER="$(awk '
    /^\[toolchain\.z42\]/ {inblock=1; next}
    /^\[/ {inblock=0}
    inblock && /^launcher/ {split($0, a, "\""); print a[2]; exit}
  ' "$REPO/versions.toml" 2>/dev/null)"
  VER="${VER:-nightly}"
  say_verbose "version: $VER (from versions.toml)"
fi
if [ "$VER" = "nightly" ]; then TAG="nightly"; else TAG="v$VER"; fi

# ── 2. host RID ───────────────────────────────────────────────────────────────
os="$(uname -s)"; arch="$(uname -m)"
case "$os/$arch" in
  Darwin/arm64)              RID="macos-arm64" ;;
  Linux/x86_64)              RID="linux-x64" ;;
  Linux/aarch64|Linux/arm64) RID="linux-arm64" ;;
  *)
    say_err "unsupported host $os/$arch"
    printf "%b\n" "  Windows: use install-z42.bat  |  supported: macos-arm64 / linux-x64 / linux-arm64" >&2
    exit 1 ;;
esac
say_verbose "rid: $RID"

# ── 3. pinned-version fast path ───────────────────────────────────────────────
if [ "$VER" != "nightly" ] && [ -f "$STAMP" ] \
    && grep -q "^$VER:$RID:" "$STAMP" 2>/dev/null; then
  say_ok "$VER / $RID already installed  (${DEST})"
  exit 0
fi

# ── 4. resolve archive + sha256 + staleness id ────────────────────────────────
_manifest_str() {
  printf '%s' "$1" | grep -o "\"$2\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" \
    | head -1 | sed -E 's/.*"[^"]*"[[:space:]]*:[[:space:]]*"([^"]*)"/\1/'
}
_rid_field() {
  local json="$1" rid="$2" field="$3"
  if command -v python3 >/dev/null 2>&1; then
    printf '%s' "$json" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    r = data.get('runtimes', {}).get('$rid', {})
    for section in ('sdk', 'runtime'):
        if section in r and isinstance(r[section], dict) and '$field' in r[section]:
            print(r[section]['$field']); sys.exit(0)
    v = r.get('$field')
    if isinstance(v, str):
        print(v)
except Exception:
    pass
" 2>/dev/null
  else
    # Fallback: old-format manifest only (field directly at rid level, no nesting).
    printf '%s' "$json" | tr -d ' \n\t' \
      | grep -o "\"${rid}\":{[^{]*}" \
      | grep -o "\"${field}\":\"[^\"]*\"" \
      | head -1 \
      | sed -E 's/"[^"]*":"([^"]*)"/\1/'
  fi
}

MANIFEST_URL="https://github.com/$SLUG/releases/download/$TAG/release-index.json"
say_verbose "fetching manifest: $MANIFEST_URL"
MANIFEST_JSON=""
MANIFEST_JSON="$(curl -fsSL "$MANIFEST_URL" 2>/dev/null)" || MANIFEST_JSON=""

ASSET=""
MANIFEST_SHA=""
WANT=""

if [ -n "$MANIFEST_JSON" ]; then
  MANIFEST_PUBLISHED="$(_manifest_str "$MANIFEST_JSON" "published")"
  ASSET="$(_rid_field "$MANIFEST_JSON" "$RID" "archive")"
  MANIFEST_SHA="$(_rid_field "$MANIFEST_JSON" "$RID" "sha256")"

  if [ -z "$ASSET" ]; then
    say_err "RID '$RID' not found in release-index.json"
    printf "%b\n" "  (this RID may not be supported — see --help for the list)" >&2
    exit 1
  fi

  WANT="$VER:$RID:${MANIFEST_PUBLISHED:-unknown}"
  if [ -f "$STAMP" ] && [ -n "$MANIFEST_PUBLISHED" ] \
      && [ "$(cat "$STAMP" 2>/dev/null)" = "$WANT" ]; then
    say_ok "already up to date  ($VER / $RID,  published ${MANIFEST_PUBLISHED})"
    exit 0
  fi
  DOWNLOAD_NOTE="[manifest]"
  say_verbose "manifest: asset=$ASSET  published=${MANIFEST_PUBLISHED:-?}"

else
  say_verbose "manifest unavailable — using SHA256SUMS fallback"
  ASSET="z42-sdk-$VER-$RID.tar.gz"

  ID=""
  if [ "$VER" = "nightly" ]; then
    ID="$(curl -fsSL "https://api.github.com/repos/$SLUG/releases/tags/nightly" 2>/dev/null \
      | grep -m1 '"published_at"' | sed -E 's/.*"published_at": *"([^"]+)".*/\1/')" || true
  else
    ID="$TAG"
  fi
  WANT="$VER:$RID:${ID:-unknown}"
  if [ -f "$STAMP" ] && [ -n "$ID" ] && [ "$(cat "$STAMP" 2>/dev/null)" = "$WANT" ]; then
    say_ok "already up to date  ($VER / $RID)"
    exit 0
  fi
  DOWNLOAD_NOTE="[SHA256SUMS fallback]"
fi

URL="https://github.com/$SLUG/releases/download/$TAG/$ASSET"

# ── 5. dry-run exit ───────────────────────────────────────────────────────────
if [ $DRY_RUN -eq 1 ]; then
  mode="portable"
  [ $SYSTEM_INSTALL -eq 1 ] && mode="managed (--system)"
  say "Dry run — no files written."
  printf "%b\n" "  version:  ${bold}$VER${normal}  ($TAG)" >&3
  printf "%b\n" "  rid:      $RID" >&3
  printf "%b\n" "  asset:    $ASSET  $DOWNLOAD_NOTE" >&3
  printf "%b\n" "  url:      $URL" >&3
  printf "%b\n" "  dest:     $DEST  ($mode)" >&3
  exit 0
fi

# ── 6. download ───────────────────────────────────────────────────────────────
say "Downloading  ${bold}$ASSET${normal}  (${TAG})  →  $DEST  $DOWNLOAD_NOTE"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
curl -fSL "$URL" -o "$TMP/$ASSET" \
  || { say_err "download failed: $URL"; exit 1; }

# ── 7. verify SHA256 ──────────────────────────────────────────────────────────
_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

if [ -n "$MANIFEST_SHA" ]; then
  got="$(_sha256 "$TMP/$ASSET")"
  say_verbose "sha256: expected=$MANIFEST_SHA  got=$got"
  [ "$MANIFEST_SHA" = "$got" ] \
    || { say_err "SHA256 mismatch for $ASSET"; exit 1; }
  say "  ${green}✓${normal} SHA256 verified"
else
  if curl -fsSL "https://github.com/$SLUG/releases/download/$TAG/SHA256SUMS" \
      -o "$TMP/SHA256SUMS" 2>/dev/null; then
    line="$(grep " $ASSET\$" "$TMP/SHA256SUMS" || grep "  $ASSET\$" "$TMP/SHA256SUMS" || true)"
    if [ -n "$line" ]; then
      want_hash="${line%% *}"
      got="$(_sha256 "$TMP/$ASSET")"
      say_verbose "sha256 (SHA256SUMS): expected=$want_hash  got=$got"
      [ "$want_hash" = "$got" ] \
        || { say_err "SHA256 mismatch for $ASSET"; exit 1; }
      say "  ${green}✓${normal} SHA256 verified  (SHA256SUMS)"
    else
      say_warn "SHA256SUMS found but no entry for $ASSET — skipping integrity check"
    fi
  else
    say_warn "SHA256SUMS not available — skipping integrity check"
  fi
fi

# ── 8. extract + install ──────────────────────────────────────────────────────
say "Extracting..."
mkdir -p "$TMP/pkg"
tar -xzf "$TMP/$ASSET" -C "$TMP/pkg"

# Restore the executable bit (GitHub Actions artifact upload/download strips it).
# Generic over the whole bin/ + the root launcher(s) so new tools (z42b, z42d,
# z42i, …) need no edits here — bin/ holds only executables in every package.
_restore_exec() {
  local d="$1" f
  for f in "$d/z42" "$d/z42.exe"; do [ -f "$f" ] && chmod +x "$f" 2>/dev/null || true; done
  [ -d "$d/bin" ] && chmod +x "$d"/bin/* 2>/dev/null || true
}

if [ $SYSTEM_INSTALL -eq 1 ]; then
  # ── Managed install ──────────────────────────────────────────────────────
  # unify-launcher-apphost (2026-06-21): the SDK launcher no longer manages
  # multiple runtimes. The package is self-contained — z42 (apphost) at the
  # root, bin/z42vm colocated, programs/launcher/launcher.zpkg, libs/ — so a
  # managed install is just an extract-in-place into $DEST (same as portable)
  # plus a PATH hint. No structured bin/launcher/runtimes split, no separate
  # launcher runtime (the apphost uses its colocated bin/z42vm). Update =
  # re-run this script: the STAMP check above no-ops a same
  # -version reinstall; a new tag re-extracts. (Multi-version support deferred.)
  rm -rf "$DEST"; mkdir -p "$DEST"
  cp -R "$TMP/pkg"/. "$DEST"/
  _restore_exec "$DEST"
  echo "$WANT" > "$STAMP"
  say_ok "Installed  ${bold}$VER${normal} / $RID  →  $DEST  (managed)"

  if [ $NO_PATH -eq 0 ]; then
    # z42 lives at the package root; z42c / z42vm in bin/ (launcher.md PATH rule).
    case ":$PATH:" in
      *":$DEST:"*) say "  \$PATH already contains ${bold}$DEST${normal}" ;;
      *)
        printf "%b\n" "" >&3
        printf "%b\n" "  ${bold}To activate z42, add to your shell profile:${normal}" >&3
        printf "%b\n" "    ${yellow}export PATH=\"$DEST:$DEST/bin:\$PATH\"${normal}" >&3
        printf "%b\n" "" >&3
        printf "%b\n" "  Then restart your shell (or run the export above), and:" >&3
        printf "%b\n" "    ${bold}z42 run <app.zpkg>${normal}" >&3
        printf "%b\n" "" >&3
        printf "%b\n" "  To update later, re-run this script." >&3
    esac
  fi

else
  # ── Portable install ─────────────────────────────────────────────────────
  rm -rf "$DEST"; mkdir -p "$DEST"
  cp -R "$TMP/pkg"/. "$DEST"/
  _restore_exec "$DEST"
  echo "$WANT" > "$STAMP"

  ENTRY="$DEST/z42"; [ -f "$ENTRY" ] || ENTRY="$DEST/bin/z42"
  say_ok "Installed  ${bold}$VER${normal} / $RID  →  $DEST  (portable)"
  say  "  entry: ${bold}$ENTRY${normal}   (add ${bold}$DEST${normal} to PATH, or run via \$REPO/.z42/z42)"
fi
