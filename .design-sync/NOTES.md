# design-sync notes — @wardnet/ui → Wardnet Design System

Project: Wardnet Design System (f1bae332-fde1-48e9-9d9d-f7f2a072b351).
Shape: storybook. Package: @wardnet/ui (source/ui). Build: `NODE_OPTIONS=--preserve-symlinks yarn workspace @wardnet/ui build` → dist/index.js.

## General learnings
- [GENERAL] Fonts: styles.css's UNLAYERED `:root` sets `--font-sans:"Inter Tight"`
  / `--font-mono:"JetBrains Mono"`, which beat theme.css's `@layer theme` "…Variable"
  names — but only the "…Variable" families ship an @font-face. So the bundle
  resolves to bare names with no font → system fallback (invisible to compare,
  since the reference falls back the same way). Fixed via `cfg.extraFonts`
  (.design-sync/fonts/aliases.css) aliasing the bare names onto the fontsource
  variable woff2.
- The `@wardnet/ui` build externalizes @wardnet/styles (CSS); the real design
  tokens + typography roles come from the storybook scrape ([CSS_FROM_STORYBOOK]),
  not ui.css. So the reference storybook build is load-bearing for styling.
- Typography (Foundations/Typography) is a scale/role *showcase* story, not a
  component export — excluded via `cfg.titleMap {"Typography": null}`.

## Re-sync risks (watch-list)
- **sb-reference is patched by hand.** `.design-sync/sb-reference/iframe.html`
  has injected bare-name @font-face rules (marker `/* ds-sync bare-name font
  alias */`) so the oracle renders the real fonts on both sides. The reference
  is gitignored + regenerated, so ANY `npx storybook build` of the reference
  DROPS this patch — RE-APPLY it via `python3 .design-sync/patch-reference-fonts.py` (idempotent)
  before grading, or every text component reads as a font mismatch.
- Decorator bundle fails on `.woff2` loader; previews render light-mode without
  the `.storybook/preview` decorator. Acceptable (theme is a data-attribute, not
  a React provider; CSS scraped, fonts shipped) — verified via compare.
- [GENERAL] [RENDER_THIN] fires for pure-SVG components (Sparkline) — the
  validator's text+paint heuristic misses SVG paint. Compare confirms Sparkline
  renders in all stories; treat this warning as a false-positive (validate still
  exits 0).

## Composite-component CSS (SegmentedTabs / FormActions)
- [GENERAL] The converter's per-preview CSS (`_preview/<Name>.css`) only captures
  a component's CO-LOCATED CSS-module styles, so components composing styles
  across files render UNSTYLED: SegmentedTabs imports `../primitives/tabs.module.css`
  (cross-dir → no `_preview` css at all); FormActions imports Button transitively
  (`_preview` css missing `button_*`). The bundle uses the dist-hashed classes
  (`_tabs_12j3q_4`, `_btn_srusb_1`, `_actions_*`) which live in `dist/ui.css`.
  FIX: `.storybook/preview.css` now `@import "@wardnet/ui/styles.css"` so the
  reference storybook's compiled CSS — hence the converter scrape (_ds_bundle.css)
  — carries the exact dist-hashed module classes globally. Every component is now
  styled regardless of import topology. (Re-sync: the preview.css import is
  committed, so a reference rebuild keeps it.)
