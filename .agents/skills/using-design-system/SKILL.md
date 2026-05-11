# Wardnet Forge — Skill

When designing for Wardnet, follow this system.

## Files to read first
1. `../..design-system/styles.css` — full token set, both themes, density variants
2. `../..design-system/primitives.jsx` — shared React primitives (`Icon`, `Sparkline`, `Donut`, `Toggle`, `StatTile`)
3. `../..design-system/data.jsx` — mock data shape; reuse names/IPs/MACs from here for consistency
4. `../..design-system/design-system.html` — visual reference; open this when in doubt
5. `../..design-system/README.md` — overview, principles, voice rules

## Always do
- **Use tokens, not hex** — `var(--accent)`, `var(--bg-card)`, `var(--line)`. Never inline `#1ED68A` directly.
- **Mono for facts** — wrap IPs, MACs, endpoints, ports, durations, hashes in `className="mono"` (JetBrains Mono).
- **Status pills shape** — green soft (`var(--accent-soft)` bg, `var(--accent-soft-ink)` text) for OK, red soft for down, neutral for stale.
- **Numbers earn the room** — page-level stats use `font-size: 32px; font-weight: 600; letter-spacing: -0.025em; font-variant-numeric: tabular-nums`.
- **Action lives on the right** — secondary/Cancel left, primary right, destructive last and always red.
- **Six principles** — every screen passes the Six Principles in `README.md` before shipping.

## Never do
- Don't introduce new accent colors. The brand is one green; status uses warn/danger/info that already exist.
- Don't use system fonts or Inter (un-Tight). Inter Tight only.
- Don't add emoji unless it's a country flag in tunnel context (the only sanctioned exception).
- Don't write "Currently", "your", "we'll" — see Voice in README.
- Don't draw SVG illustrations. Use placeholders + monospace explainers if imagery is missing.
- Don't break the floating sidebar. The shell never moves.

## Building a detail page (forms)
A detail page is a **stack of independent cards**, each flipping read ↔ edit on its own.

1. Page header: H1 + status pill + breadcrumb back to the index page
2. Cards top-to-bottom: Identity (always read-only) → Settings → Domain-specific (DNS filtering, Routing, etc) → Network. Order = least to most situational.
3. Each editable card has an `Edit` button top-right. While editing: card shows inputs, a Cancel/Save row docks at the bottom-right of *that card only*.
4. Fields: 11px uppercase label above the value/input. 2-col grid (1-col on mobile). Long fields (configs, descriptions) span both columns.
5. Read-only fields (MAC, first-seen, manufacturer) **never** enter edit mode — same render in both modes.
6. Validation: inline, on blur. Red 1px border + 12px error text below. Save disabled while invalid.
7. Destructive actions (Delete, Revoke) sit **outside cards**, at the bottom of the page in a danger-toned button.
8. Keyboard: `⌘+Enter` saves the active card, `Esc` cancels.
9. Reuse `<FieldRead>` / `<FieldInput>` patterns from `design-system.html` §09 — don't roll new field markup.

## Adding a chart
Match the chart to the question:
- **"How much, over time?"** → throughput line chart with soft-fill area. 2 series max: Download = `--accent`, Upload = `--info`. Always include a window pill group (1h / 6h / 24h / 48h / 12mo).
- **"Is it trending?"** → sparkline inside a stat tile. No axes, no labels.
- **"What's the breakdown?"** → donut (≤4 categories, headline number in middle) or stacked horizontal bar (ranked top-N).

Rules: 4 horizontal hairline gridlines (`--line`, `2 4` dasharray), no vertical grid. Y-axis = 4 mono labels with a unit. Empty state = "Collecting…", never a flat-zero line. Tooltip = `--bg-card` + `--shadow-pop`, mono timestamp, bold values. See `design-system.html` §10 for examples.

## Adding a new screen
1. Drop it into `screens.jsx` (or `detail-screens.jsx` for sub-routes)
2. Header pattern: `<h1>Title</h1>` + optional status pill + optional right-aligned primary action
3. First card answers "is it healthy?" — status pill, headline number, supporting stats
4. Use `<StatTile>`, `<Sparkline>`, list rows, and form sections from primitives — don't roll new components without checking
5. Add to sidebar nav in `app.jsx`
6. Mirror it in `mobile.html` if user-facing

## Adding a new component
1. Build it as a Babel file in the project root
2. Export it via `Object.assign(window, { MyComponent })` at the bottom — components don't share scope across `<script type="text/babel">` blocks
3. Use `let myStyles = {}` style objects, never bare `const styles = {}` (collisions are fatal)
4. Visualize it in `design-system.html` under section 05 Components

## Adding tweaks
The Tweaks panel (`tweaks-panel.jsx`) already exposes theme / density / sidebar style / accent. New tweaks go through the host's edit-mode protocol — see `tweaks-panel.jsx` and the `TWEAK_DEFAULS` block in `app.jsx`.
