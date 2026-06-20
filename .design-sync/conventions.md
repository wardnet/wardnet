# Wardnet design system — how to build with it

Dense, technical admin UI. Warm **Paper** surfaces, deep near-black **Ink** text,
a single **Emerald** accent. Compose the real `@wardnet/ui` components below; for
your own layout glue, style with the **design tokens (CSS variables)** — NOT
arbitrary Tailwind utility classes.

## Setup / wrapping

No React provider or theme context is required — components read CSS custom
properties that ship in the linked `styles.css`. Just render them.

- **Light is the default.** For dark mode, set `data-theme="dark"` on any
  ancestor element (e.g. the page root); every token flips automatically.
- Fonts (Inter Tight, JetBrains Mono) ship with the bundle — nothing to load.

## Styling idiom — tokens, not utility classes

The design pane has **no Tailwind runtime**, so only a handful of utility classes
were captured (`text-accent`, `text-danger`, `flex`, `grid`, `gap-*`,
`items-center`). Do **not** reach for `bg-card`, `text-ink`, `rounded-lg`,
`p-4`, etc. — they will not resolve and your element ships unstyled. Instead:

**1. Style with CSS variables** (these all ship and are the styling API):

| Concern | Tokens (`var(--…)`) |
|---|---|
| Surfaces | `--bg` `--bg-elev` `--bg-sunken` `--bg-card` `--line` `--line-strong` |
| Text (ink) | `--ink` (primary) `--ink-2` (secondary) `--ink-3` (muted/labels) `--ink-4` (faint) |
| Brand | `--accent` (emerald) `--accent-soft` `--accent-soft-ink` |
| Status | `--danger`/`--danger-soft`, `--warn`/`--warn-soft`, `--info`/`--info-soft` |
| Radius | `--radius-sm` (6) `--radius` (10) `--radius-lg` (14) `--radius-xl` (20) |
| Elevation | `--shadow-card` (resting) `--shadow-pop` (overlays) |
| Type family | `--font-sans` `--font-mono` |

```jsx
<div style={{ background: "var(--bg-card)", border: "1px solid var(--line)",
  borderRadius: "var(--radius-lg)", boxShadow: "var(--shadow-card)", padding: 18 }}>
```

**2. Typography → the `<Text>` / `<Heading>` primitive** (never raw font sizing):

```jsx
<Text variant="label">DEVICES</Text>            {/* uppercase tracked ink-3 */}
<Text variant="body">Body copy.</Text>          {/* default 13px */}
<Heading level={2}>Section header</Heading>
<Text variant="metric">12,481</Text><Text variant="metric-unit">ms</Text>
```
`variant` bakes size+weight+colour+element; override per call with `size`
("2xs"…"4xl"), `weight` ("normal"|"medium"|"semibold"|"bold"), `color`
("accent"|"danger"|"ink-3"…), `as`. Variants: `label, body, body-strong,
caption, micro, metric, metric-unit, mono, h1, h2, h3`. (Equivalent CSS classes
`t-label`, `t-size-sm`, `t-weight-semibold` also ship if you can't use the
primitive.) `role` stays the native ARIA attribute.

**3. Prefer the components** — they are pre-styled; don't restyle them.
Surfaces: `Card` (`CardHeader/CardTitle/CardContent/CardFooter`), `StatTile`,
`Banner`, `Pill`. Forms: `Button`, `Input`, `Textarea`, `Select`, `Combobox`,
`Toggle`, `Field`, `Label`, `Form`. Overlays: `Modal`, `AlertModal`, `Drawer`,
`Popover`, `DropdownMenu`. Nav/data: `Tabs`, `SegmentedTabs`, `Sparkline`.
Brand: `Logo`. One emerald `Button` (primary) per view; everything else
ink/line.

## Where the truth lives

- **`styles.css`** — the single linked stylesheet: it `@import`s the tokens,
  fonts, and component styles. Read it for the exact token values.
- **`components/<group>/<Name>/<Name>.prompt.md`** + `.d.ts` — per-component
  API and usage. Read these before composing a component.

## One idiomatic example

```jsx
<Card>
  <CardHeader><CardTitle>Blocked queries</CardTitle></CardHeader>
  <CardContent style={{ display: "flex", flexDirection: "column", gap: 8 }}>
    <Text variant="metric">12,481</Text>
    <Text variant="caption">last 24h</Text>
    <Button>View log</Button>
  </CardContent>
</Card>
```
