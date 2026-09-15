#!/usr/bin/env bash
# install-z42.sh — repository bootstrap (macOS / Linux).
#
# Installs the z42 toolchain version pinned in versions.toml ([toolchain.z42].launcher)
# into <repo>/.z42 — what `xtask` and the build scripts use as their seed — without
# touching your PATH. Re-run to update; an unchanged download is skipped.
#
#   ./scripts/install-z42.sh                    # pinned version → <repo>/.z42
#   ./scripts/install-z42.sh --version 0.6.0    # override the version
#   ./scripts/install-z42.sh --force            # reinstall even if up to date
#
# The install logic lives in scripts/install/install.sh (the user-facing installer,
# which installs into ~/.z42 and sets up PATH); this script only supplies repo defaults.
# Any option of that script can be passed through.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(awk '
  /^\[toolchain\.z42\]/ { inblock = 1; next }
  /^\[/                 { inblock = 0 }
  inblock && /^launcher/ { split($0, a, "\""); print a[2]; exit }
' "$REPO/versions.toml" 2>/dev/null || true)"

exec sh "$REPO/scripts/install/install.sh" \
  --dest "$REPO/.z42" --version "${VERSION:-nightly}" --no-modify-path "$@"
