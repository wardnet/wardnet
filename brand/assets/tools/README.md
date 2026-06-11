# Wardnet — logo build tools

Scripts for regenerating distributable logo assets from the editable masters.

---

## `build-svg.js` — convert text to vector paths

The editable lockups in `assets/src/` use **live text** in Archivo + Hanken Grotesk.
They only render correctly where those fonts are installed. For distribution you
want **outlined** versions, where the letters are baked into `<path>` data and the
file is font-independent. This script reads every SVG in `src/`, replaces the
`<text>` elements with outlined paths, and writes the results to `dist/` — all
other markup (the mark, transforms, viewBox, etc.) is preserved exactly as-is.

### Run it

```bash
# from brand/assets/tools/
yarn install          # one-time
yarn build-svg
```

First run auto-downloads the two font files into `tools/fonts/`. After that it's offline.
Output: all SVGs from `src/` with text outlined, written to `dist/`.

### When to re-run

Any time you edit a source SVG in `src/` (colors, positions, text, the mark), just
re-run the script. It reads the source files directly — there is no separate config
block to keep in sync.

### How it works (the short version)

The script parses each source SVG as XML, finds every `<text>` element, and replaces
it with one or more `<path>` elements whose `d` attribute contains the glyph outlines.
The surrounding markup — including parent `<g transform>` wrappers — is left untouched,
so transforms and layout are preserved.

Glyph placement uses [`opentype.js`](https://github.com/opentypejs/opentype.js):

1. `font.stringToGlyphs(text)` → the glyph for each character
2. for each glyph: `glyph.getPath(x, baseline, fontSize)` → its outline as SVG path data
3. advance `x` by `glyph.advanceWidth * fontSize/unitsPerEm + letterSpacing`, plus
   `font.getKerningValue(prev, glyph)` between pairs
4. `<tspan>` children (e.g. WARD in ink, NET in emerald) become separate `<path>` elements,
   each starting at the `endX` of the previous run

The **tagline fit** (`textLength` + `lengthAdjust="spacing"`) is solved the same way the
browser does it: measure the natural width, then set
`letterSpacing = (targetWidth − naturalWidth) / (glyphCount − 1)`, with one correction pass.

---

## Alternatives (no Node)

Outlining is a standard operation — any of these work too:

| Tool | How |
|------|-----|
| **Adobe Illustrator** | Select the text → **Type ▸ Create Outlines** (⇧⌘O) → save as SVG |
| **Inkscape** (free) | Select text → **Path ▸ Object to Path** (⇧⌘C). CLI: `inkscape in.svg --export-text-to-path --export-plain-svg=out.svg` |
| **Figma** | Select text → right-click → **Outline stroke** / flatten → export SVG |
| **fontTools / picosvg** (Python) | scriptable, good for pipelines |

The Node script is here so it's **repeatable and version-controlled** — same input, same
bytes out, no manual steps to forget.

---

## `build-png.js` — render SVG → PNG at a given size

SVGs are the source of truth. When a context needs raster (favicons, store icons, email,
Office docs), generate a PNG at the size you need — don't keep a pile of pre-rendered files.

```bash
# from brand/assets/tools/
node build-png.js <input.svg> <width> [output.png]

# examples
node build-png.js ../dist/wardnet-icon.svg 512
node build-png.js ../dist/wardnet-logo-light.svg 800 logo-800.png
node build-png.js ../dist/wardnet-icon.svg 32 favicon-32.png
```

If no output path is given, the PNG is written alongside the SVG with a `-<width>` suffix
(e.g. `dist/wardnet-icon-512.png`).

Always use the **outlined** files from `dist/` as input — the `src/` files use live text
and will render incorrectly if the fonts are not installed on the build machine.

---

## Files at a glance

```
brand/assets/
  src/      editable masters (text as text — needs fonts to render correctly)
  dist/     outlined, font-free — use these for embedding / PNG export
  tools/
    build-svg.js     reads src/, writes outlined SVGs to dist/
    build-png.js     renders a dist/ SVG to PNG at a given width
    package.json     yarn dependencies (opentype.js, @xmldom/xmldom, @resvg/resvg-js)
    fonts/           auto-downloaded TTFs (git-ignored)
```
