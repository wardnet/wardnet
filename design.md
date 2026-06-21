# Wardnet Design System

A self-contained reference for the Wardnet visual language — brand, colour,
typography, spacing, elevation, motion, and the component library. Use this to
reproduce the Wardnet look in new designs. Source of truth lives in
`source/styles/` (`tokens.ts` → `styles.css` → `theme.css` / `typography.css`)
and the React primitives in `source/ui/`. Typography rationale:
`docs/adr-typography-scale-and-roles.md`.

---

## 1. Brand

Wardnet is a **self-hosted network-privacy gateway** — a calm, dense, technical
admin aesthetic. Three brand pillars:

- **Paper** — warm off-white surfaces (not stark white).
- **Ink** — deep near-black navy for text and dark chrome.
- **Emerald** — the single accent (`#12b981`), used sparingly for primary
  actions, active state, and the first chart series.

Voice: precise, compact, trustworthy. Generous hairlines over heavy borders;
soft shadows over hard ones; one accent, never a rainbow.

**Typefaces**
- Sans: **Inter Tight** (`--font-sans`) — UI + headings.
- Mono: **JetBrains Mono** (`--font-mono`) — MAC/IP/endpoint identifiers, code,
  KPI tickers. Mono uses the `zero` font-feature (slashed zero).
- Body uses `font-feature-settings: "ss01", "cv11"` and a slight global
  `letter-spacing: -0.005em`.

---

## 2. Colour

Tokens are CSS custom properties that flip between light and dark via
`[data-theme="dark"]`. Always reference the **token**, never a raw hex.

### Surfaces

| Token | Light | Dark | Use |
|-------|-------|------|-----|
| `--bg` | `#f4f3ee` Paper | `#0c1022` | App background |
| `--bg-elev` | `#ffffff` | `#1b2140` | Raised surface |
| `--bg-sunken` | `#eceae2` | `#080b16` | Recessed (footers, wells) |
| `--bg-card` | `#ffffff` | `#1b2140` | Card surface |
| `--line` | `#e6e4dc` | `#2a3155` | Hairline border / divider |
| `--line-strong` | `#d6d3c8` | `#38406a` | Stronger divider |

### Ink (text)

| Token | Light | Dark | Use |
|-------|-------|------|-----|
| `--ink` | `#11152b` | `#ecedf2` | Primary text |
| `--ink-2` | `#5b6178` Slate | `#c2c6d4` | Secondary text |
| `--ink-3` | `#8a90a6` Mist | `#8a90a6` | Tertiary / labels / captions |
| `--ink-4` | `#abb1c4` | `#5b6178` | Faint / disabled |

### Sidebar / dark chrome (constant across themes — always on Ink)

| Token | Value | Use |
|-------|-------|-----|
| `--side-bg` | `#11152b` (light) / `#080b16` (dark) | Sidebar background |
| `--side-line` | `#2a3155` / `#1c2444` | Sidebar divider |
| `--side-ink` | `#c9cce0` / `#b6bbcf` | Sidebar text |
| `--side-ink-2` | `#7e859b` | Sidebar muted |
| `--side-ink-active` | `#ffffff` | Active sidebar item |
| `--side-active-bg` | `#1b2140` | Active item background |

### Brand & status

| Token | Light | Dark | Use |
|-------|-------|------|-----|
| `--accent` | `#12b981` Emerald | (same) | Primary action, active, chart-1 |
| `--accent-ink` | `#11152b` | `#11152b` | Text/marks on an emerald surface |
| `--accent-soft` | `#e7f8f0` | `#103d2a` | Tinted accent surface |
| `--accent-soft-ink` | `#0e9266` | `#57e0a3` | Text on tinted accent |
| `--warn` | `#f1b13b` | (same) | Warning |
| `--warn-soft` / `-ink` | `#fcefcf` / `#6b4a05` | `#3a2d10` / `#f1b13b` | Warning surface/text |
| `--danger` | `#e5484d` | (same) | Error / destructive |
| `--danger-soft` / `-ink` | `#fde8e8` / `#8a1a1f` | `#3a1518` / `#ff8a8e` | Danger surface/text |
| `--info` | `#4d8df6` | (same) | Info / chart-2 |
| `--info-soft` | `#dde9fe` | `#122749` | Info surface |

### Chart palette

Categorical charts cycle **accent → info → warn → ink-3** (`--chart-1..4`).
Two-series time charts use 1 + 2 (e.g. Download / Upload). Never invent a new
chart palette — extend the cycle.

---

## 3. Typography

Two tiers: a dense numeric **scale** + named semantic **variants** layered on
top. One scale across apps and components (no 13-vs-14 split).

### Scale (rem @ 16px root, with paired line-heights)

| Token | px | rem | line-height |
|-------|----|-----|-------------|
| `2xs` | 11 | .6875 | 1.3 |
| `xs` | 12 | .75 | 1.35 |
| `sm` | 13 | .8125 | 1.5 |
| `base` | 14 | .875 | 1.5 |
| `lg` | 16 | 1 | 1.4 |
| `xl` | 18 | 1.125 | 1.3 |
| `2xl` | 22 | 1.375 | 1.2 |
| `3xl` | 26 | 1.625 | 1.15 |
| `4xl` | 32 | 2 | 1.05 |

Default body text is **sm (13px)**. Nothing exceeds `4xl` (32px). No responsive
type sizes.

### Variants (named voices — each bakes size + weight + tracking + colour)

| Variant | Size | Weight | Tracking / transform | Colour | Use |
|---------|------|--------|----------------------|--------|-----|
| `label` | xs (12) | 500 | uppercase, 0.06em | ink-3 | card titles, stat labels, table headers |
| `body` | sm (13) | 400 | — | ink | default UI / prose |
| `body-strong` | sm (13) | 600 | — | ink | inline emphasis |
| `caption` | xs (12) | 400 | — | ink-3 | field help / secondary |
| `micro` | 2xs (11) | 500 | — | ink-3 | tiny meta |
| `metric` | 4xl (32) | 600 | −0.03em, lh 1.05 | ink | KPI values |
| `metric-unit` | xl (18) | 500 | — | ink-3 | unit beside a metric |
| `mono` | sm (13) | 400 | `font-mono` | ink | MAC / IP / endpoints |
| `h1` | 2xl (22) | 600 | −0.02em | ink | page / topbar title |
| `h2` | xl (18) | 600 | −0.01em | ink | section header |
| `h3` | lg (16) | 600 | — | ink | modal / card-block title |

### Primitive API (`@wardnet/ui`)

One `<Text>` primitive + a thin `<Heading level={1|2|3}>` alias
(≡ `<Text variant={"h"+level}>`). The prop is **`variant`**, not `role` — `role`
stays the native ARIA attribute and passes straight through.

```tsx
<Text variant="label">DEVICES</Text>          // baked label voice
<Text variant="body" weight="semibold">…</Text> // override one property
<Text variant="h2" as="div">Section</Text>      // voice decoupled from element
<Text size="lg" weight="medium">…</Text>        // off-variant one-off
<Text variant="label" color="danger">…</Text>   // recolour via utility
<Text variant="body" role="alert">…</Text>      // ARIA role passes through

interface TextProps {
  variant?: Variant;   // bundle: size + weight + colour + default element
  size?:    Size;      // "2xs"|"xs"|"sm"|"base"|"lg"|"xl"|"2xl"|"3xl"|"4xl"
  weight?:  Weight;    // "normal"|"medium"|"semibold"|"bold"  (400/500/600/700)
  color?:   Color;     // ink | ink-2..5 | accent | danger | warn | info | …
  as?:      ElementType;
  className?: string;  // colour utilities still valid here
  // …plus native HTMLAttributes incl. role (ARIA), aria-*, data-*
}
```

**Rules.** Reach for a **variant** (named voice) first; use `size`/`weight` props
for off-variant one-offs. Don't write raw `text-*` / `font-*` size/weight
utilities in markup — colour utilities (`text-danger`, `text-ink-3`) are kept and
override a variant's baked colour.

---

## 4. Spacing, radius, density

- **Density unit** `--pad: 18px` — default card / section padding.
- **Row height** `--row-h: 52px` — table/list rows.
- Common gaps: 6 / 8 / 10 / 12 / 14 px. Section rhythm in multiples of `--pad`.

**Radius**

| Token | Value | Use |
|-------|-------|-----|
| `--radius-sm` | 6px | inputs, pills, small chips |
| `--radius` (md) | 10px | buttons, inner blocks |
| `--radius-lg` | 14px | cards, stat tiles |
| `--radius-xl` | 20px | modals, large surfaces |

---

## 5. Elevation & motion

**Shadows** — soft, low-contrast.
- `--shadow-card`: `0 1px 0 rgba(20,24,32,.04), 0 1px 2px rgba(20,24,32,.04)` (cards).
- `--shadow-pop`: `0 16px 40px -12px rgba(15,20,40,.18), 0 2px 6px rgba(15,20,40,.06)` (popovers, modals, dropdowns).
- Dark mode deepens both (rgba black).

**Motion**
- Durations: `--duration-snap 120ms` (hover/press), `--duration-slide 200ms`
  (open/close), `--duration-stream 600ms` (data/stream).
- Easing: `--ease-default: cubic-bezier(.2,.8,.2,1)` ("soft out").

---

## 6. Component library (`@wardnet/ui`)

Domain-agnostic React primitives (shadcn/Radix-adjacent), styled with the tokens
above. Feature compositions (LoginForm, AppHeader, …) live one layer up in
`@wardnet/web`.

- **Surfaces:** `Card` (Header/Title/Subtitle/Action/Content/Footer), `StatTile`
  (KPI tile — label / metric value / unit / sub / bar / sparkline / pill),
  `Banner`, `Pill` (ok/warn/down/info/ghost).
- **Forms:** `Button`, `Input`, `Textarea`, `Select`, `Combobox`, `Toggle`,
  `Field`, `Label`, `Form` (+ `Validator`), `FormActions`.
- **Overlays:** `Modal`, `AlertModal`, `Drawer`, `Popover`, `DropdownMenu`.
- **Navigation / data:** `Tabs`, `SegmentedTabs`, `Sparkline`.
- **Typography:** `Text`, `Heading` (see §3).
- **Brand:** `Logo`.

**Conventions**
- Card titles, stat labels, and table headers all share the **`label`** voice
  (small, uppercase, tracked, ink-3) — one voice, never re-stated.
- KPI numbers use the **`metric`** voice with the unit in **`metric-unit`**.
- One accent (emerald) for the primary action per view; everything else is
  ink/line. Status colour only for status (warn/danger/info).
- Hairline (`--line`) dividers, not boxes. `--shadow-card` for resting cards,
  `--shadow-pop` for floating layers.

---

## 7. Layout archetype

A typical admin screen: dark **Ink sidebar** (constant chrome) + light **Paper**
content area; a topbar with an `h1` page title and breadcrumb; a grid of
`StatTile` KPIs; cards with `label`-voiced headers containing tables (`label`
headers, `sm` body rows, `mono` for identifiers) or charts (accent→info→warn→ink-3).
