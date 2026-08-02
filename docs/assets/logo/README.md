# z42 — Brand assets

The z42 logo: a normal geometric **z** inside a square frame.

## The idea

- **z** — the letter of the name; the last letter, the final evolution.
- **the square frame** — a square *is* "squared" (z²), and its **four sides** are the **4** of z² = 4, and of z**4**2.

So the whole mark spells `z² = 4 → z42` with a single letter and a single box — no superscript needed.

## Colors

| Token | Hex | Use |
|-------|-----|-----|
| Ink | `#191C2B` | Primary mark on light backgrounds |
| Paper | `#FFFFFF` | Reversed mark on dark backgrounds |
| Accent (vermilion) | `#F0522E` | Brand accent — optional accent frame, UI highlights |

## Layout

```
docs/assets/logo/
├── svg/   vector marks (the source of truth)
├── png/   raster renders of the mark
└── web/   standards-compliant website icon package
```

### `svg/` — vector marks

| File | What | Where to use |
|------|------|--------------|
| `z42-logo.svg` | Ink outline mark, transparent | Light backgrounds, docs, sites |
| `z42-logo-reverse.svg` | White outline mark, transparent | Dark backgrounds |
| `z42-logo-accent.svg` | Vermilion frame + ink z | Brand-colored contexts |
| `z42-glyph.svg` | The z only, `currentColor` | Inline text, monochrome |
| `z42-icon.svg` | Filled tile (dark square, white z) | App icon — reads on any background |

### `png/` — raster renders

| File | What |
|------|------|
| `z42-logo-512.png` | Outline mark, transparent |
| `z42-logo-reverse-512.png` | White outline mark, transparent |

### `web/` — website icon package (standard)

| File | Purpose |
|------|---------|
| `favicon.ico` | Legacy/broad support, multi-size (16/32/48) |
| `favicon.svg` | Modern scalable favicon |
| `favicon-16.png` / `favicon-32.png` / `favicon-48.png` | PNG favicons |
| `apple-touch-icon.png` (180) | iOS home screen (full-bleed, platform masks it) |
| `icon-192.png` / `icon-512.png` | PWA / Android manifest icons |
| `icon-maskable-512.png` | Android adaptive/maskable icon (glyph in safe zone) |
| `site.webmanifest` | PWA manifest (icons, theme/background color) |
| `og-image.png` (1200×630) | Open Graph / Twitter social share card |
| `HEAD.html` | Ready-to-paste `<head>` link/meta snippet |

**Deploy:** copy the contents of `web/` to your site root (or adjust the paths in
`web/HEAD.html` / `web/site.webmanifest`), then paste `web/HEAD.html` into your page `<head>`.

## Usage notes

- **Small sizes** (≤ ~24px): use the **filled icon** (`z42-icon.*`). The thin outline frame thins out below that size.
- **Clear space**: keep at least the frame's stroke width of padding around the mark.
- **Don't**: shear or slant the z, add a drop shadow, recolor the z outside the palette, or stretch the square.

## Regenerating the PNGs

The PNGs are rendered from the SVGs with headless Chromium (transparent background, exact pixel sizes). Re-run the render step if the SVG source changes.
