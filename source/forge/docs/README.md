# Wardnet Forge — docs

This directory is **docs-only**. It holds the visual studio for the Wardnet
Forge design system plus the React mocks the studio was originally extracted
from. Nothing here is shipped to consumers — no app build pulls from
`source/forge/docs/` and no published artefact references it. It exists as
reference material for engineers porting components from the mocks into the
real packages.

## Canonical sources

The studio is *about* the design system; it is not the design system.
The canonical artefacts live one level up in `source/forge/`:

| Canonical | What it is |
|---|---|
| `../styles.css` | Forge CSS — tokens (`:root`, `[data-theme="dark"]`) plus the component classes (`.card`, `.btn--primary`, `.toggle`, …). Single source of truth for web rendering. |
| `../src/tokens.ts` | Platform-neutral token values (brand, status, radius, density, font). The TS object the CSS variables manifest. Consumed directly by non-CSS surfaces (charts picking a runtime colour, future React Native primitives). |

The Radix-bound React primitives that wrap these classes for the admin app
live in `source/admin-app/forge-web/` (`@wardnet/forge-web`). They're
internal to the admin product, not part of `@wardnet/forge`.

## Files

| File | What it is |
|---|---|
| `design-system.html` | The studio — a static reference page covering tokens, type, components, and voice. Hand-rendered HTML, opened directly in a browser. Loads `../styles.css` so token changes in the canonical CSS show up here on reload. |
| `data.jsx` | Mock data (devices, tunnels, leases, logs, etc.) the screen mocks render against. |
| `screens.jsx`, `detail-screens.jsx` | Original section + detail screen mocks. Reference for component composition; not wired to a build. |
| `tweaks-panel.jsx` | The theme / density / sidebar / accent picker mock from the studio. |
| `logo/` | Brand logo source files (PNG, XCF). |

The `.jsx` files are **not** built. They predate the package split and are
kept verbatim as a porting reference. When a primitive is reborn in
`forge-web/`, the corresponding mock here is the source of truth for
behaviour, density, and ARIA wiring until the port is complete.

## Studio rendering

`design-system.html` is rendered as a static file — open it in a browser, no
dev server required. It links `../styles.css`, so the tokens and component
classes shown in the studio are the same ones the apps consume; there is no
parallel CSS to drift.

**Long-term intent:** the studio should render its component examples
against the real package builds (`@wardnet/forge`'s CSS plus
`@wardnet/forge-web`'s React primitives mounted into the page) so a
component change in code is immediately visible in the studio. Until that
work lands, the studio is hand-rendered HTML — markup written by hand to
mirror what the components look like, with the canonical CSS doing the
visual heavy lifting.
