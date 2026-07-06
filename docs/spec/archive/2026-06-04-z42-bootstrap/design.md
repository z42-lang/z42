# Design: z42-bootstrap

## Resolution / URL (reuse launcher install convention)
- tag = (launcher=="nightly") ? "nightly" : "v"+launcher
- ver = launcher value ("nightly" | "0.1.0")
- asset = z42-<ver>-<rid>.tar.gz   (windows: .zip)
- url = https://github.com/z42-lang/z42/releases/download/<tag>/<asset>
- inner extracted dir = z42-<ver>-<rid>-release/  → contents moved into .z42/

## Staleness check (stamp = .z42/.bootstrap-stamp, "ver:rid:id")
- nightly: id = release `published_at` (changes on each republish) → re-download
  when stamp differs.
- pinned:  id = tag (immutable) → skip if stamp matches.

## RID detection
uname -s/-m → macos-arm64 / linux-x64 / linux-arm64. Windows handled by .bat
(only the 9 vendor-supported RIDs; see project_supported_platforms).

## Trampoline location (post launcher-at-package-root)
The downloaded package has `z42` at its root, so `.z42/z42` is the entry.
(Until a Stream-4-layout nightly is published, current nightly has bin/z42 —
the bootstrap extracts whatever layout the release ships; the new-layout
nightly is the prerequisite for the final script replacement, not for the
bootstrap download mechanism itself.)

## Portability
sha tool: prefer `sha256sum`, fall back to `shasum -a 256`. Download: `curl -fSL`.
