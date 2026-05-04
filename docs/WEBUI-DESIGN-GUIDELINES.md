# Wardnet Admin UI — Design Guidelines

> Single source of truth for the visual design and interaction patterns of the Wardnet admin UI. Both humans and AI coding agents should treat this as authoritative when building or refactoring UI. When the existing codebase conflicts with this document, the document wins — file an issue and migrate.

Version 1.0 · Last updated 2026-05-04

---

## Contents

1. [Principles](#1-principles)
2. [Tokens](#2-tokens)
3. [Components](#3-components)
4. [Patterns](#4-patterns)
5. [Page templates](#5-page-templates)
6. [Content & microcopy](#6-content--microcopy)
7. [Iconography](#7-iconography)
8. [Accessibility](#8-accessibility)
9. [Known inconsistencies to fix](#9-known-inconsistencies-to-fix)

---

## 1. Principles

**Flat and quiet.** No gradients, drop shadows (except functional focus rings), or decorative effects. Surfaces are flat, borders are 0.5–1px, and color is used sparingly for meaning.

**One primary action per surface.** Each card, modal, page header, or empty state should have at most one filled brand-green button. Everything else is secondary, tertiary, or destructive.

**State is read-only by default; transformations are explicit.** A status indicator must look obviously non-interactive. When state becomes actionable (e.g. "Update available"), the element transforms into a button — same slot, different affordance.

**Destructive actions are visually distinct and never sit beside primary actions without separation.** Use red treatment (text + border, or text on soft red bg). Group them in a "danger zone" sub-block when they share a card with safe actions.

**Sentence case everywhere.** Headings, buttons, status labels, table headers, tab labels. Never Title Case, never ALL CAPS, never inconsistent casing within a column (no "Active" vs "active" mixed).

**Content drives layout.** Cards expand to their content. Tables truncate gracefully. Illustrations stay small. Do not pad content to fill space.

---

## 2. Tokens

Provide all tokens as CSS custom properties on `:root`. Dark mode mirrors via `[data-theme="dark"]`.

### 2.1 Color

```css
:root {
  /* Surface */
  --color-bg-page:        #f4f5f6;
  --color-bg-surface:     #ffffff;     /* cards */
  --color-bg-subtle:      #f1f2f4;     /* metric tiles, inset rows, danger zone block */

  /* Text */
  --color-text-primary:   #0f1419;
  --color-text-secondary: #5a6471;
  --color-text-tertiary:  #8a929c;
  --color-text-on-brand:  #ffffff;

  /* Border */
  --color-border-subtle:  rgba(15, 20, 25, 0.08);
  --color-border-default: rgba(15, 20, 25, 0.15);
  --color-border-strong:  rgba(15, 20, 25, 0.25);

  /* Brand / primary */
  --color-brand:          #1d9e75;
  --color-brand-hover:    #178a64;
  --color-brand-active:   #0f6e56;

  /* Success (badges, positive states) */
  --color-success-fg:     #0f6e56;
  --color-success-bg:     #dcf3e8;
  --color-success-border: rgba(15, 110, 86, 0.25);

  /* Danger (destructive actions, errors) */
  --color-danger-fg:      #b1241f;
  --color-danger-bg:      #fbeaea;
  --color-danger-border:  rgba(177, 36, 31, 0.3);

  /* Warning (cautionary callouts) */
  --color-warning-fg:     #8a5a00;
  --color-warning-bg:     #fdf3dd;
  --color-warning-border: rgba(138, 90, 0, 0.25);

  /* Neutral (informational badges, "External", "Down") */
  --color-neutral-fg:     #4a5260;
  --color-neutral-bg:     #ecedef;
  --color-neutral-border: rgba(74, 82, 96, 0.2);
}
```

**Rules**
- Never use raw hex in components. Always reference a token.
- Brand green is reserved for the primary action. Do not use it for status badges (success has its own muted tone).
- Soft semantic backgrounds (`*-bg`) are for badges and alert blocks. Never use them as page backgrounds.

### 2.2 Typography

```css
:root {
  --font-sans: "Inter", system-ui, -apple-system, sans-serif;
  --font-mono: "JetBrains Mono", ui-monospace, monospace;

  --text-xs:   12px;   /* metadata, badge text */
  --text-sm:   13px;   /* body, table cells, button labels */
  --text-base: 14px;   /* default body */
  --text-md:   15px;   /* metric values, important inline values */
  --text-lg:   16px;   /* card section headers (h2) */
  --text-xl:   20px;   /* page-level subheaders */
  --text-2xl:  24px;   /* page titles (h1) */

  --weight-regular: 400;
  --weight-medium:  500;   /* the only "bold" weight in the UI */

  --leading-tight:  1.3;
  --leading-normal: 1.5;
  --leading-relaxed: 1.65; /* paragraph copy */
}
```

**Rules**
- Two weights only: 400 regular, 500 medium. Never 600 or 700 — they read heavy in this UI.
- Use `var(--font-mono)` for IPs, MAC addresses, version strings, hex IDs, and any other identifier the user might copy.
- Page title (h1) is 24px/500. Card section header (h2) is 16px/500. Same weight, different size.
- Never go below 12px.

### 2.3 Spacing

Use rems for vertical rhythm (component-to-component) and pixels for component-internal gaps.

```css
:root {
  --space-1:  4px;
  --space-2:  8px;
  --space-3:  12px;
  --space-4:  16px;
  --space-5:  20px;
  --space-6:  24px;
  --space-8:  32px;

  --gap-card-inner:    20px;       /* card padding (vertical) */
  --gap-card-inline:   24px;       /* card padding (horizontal) */
  --gap-card-stack:    12px;       /* between cards on the same page */
  --gap-section:       16px;       /* between groups inside a card */
  --gap-row:           14px;       /* between table rows (visual) */
}
```

### 2.4 Radius & borders

```css
:root {
  --radius-sm:   6px;    /* badges, small chips */
  --radius-md:   8px;    /* buttons, inputs, alert blocks */
  --radius-lg:   12px;   /* cards */
  --radius-pill: 999px;  /* status badges, segmented tabs */

  --border-width: 0.5px;
}
```

**Rules**
- Never use rounded corners on single-sided borders. Single-side borders (`border-top`, `border-left`) require `border-radius: 0`.
- Cards use `--radius-lg`. Buttons and inputs use `--radius-md`. Pills (badges, tabs) use `--radius-pill`.

### 2.5 Motion

```css
:root {
  --motion-fast:    120ms ease-out;   /* hover, focus */
  --motion-medium:  200ms ease-out;   /* state transitions */
  --motion-slow:    400ms ease-out;   /* page-level changes */
  --motion-skeleton: 1400ms cubic-bezier(0.4, 0, 0.6, 1) infinite;
}
```

---

## 3. Components

### 3.1 Button

**Anatomy**: optional leading icon (14px), label, optional trailing icon. Height 32px standard, 28px small (table inline), 36px large (empty-state CTA).

**Variants**

| Variant | Use for | Style |
|---|---|---|
| `primary` | The single most important action on a surface | Filled `--color-brand`, white text, no border. Hover: `--color-brand-hover`. |
| `secondary` | Non-destructive utilities, "Check now", "Edit" | Transparent bg, `0.5px solid --color-border-default`, primary text. Hover: `--color-bg-subtle`. |
| `destructive` | Restart, delete, restore, revoke | Transparent bg, `0.5px solid --color-danger-border`, `--color-danger-fg` text. Hover: `--color-danger-bg`. |
| `destructive-soft` | Inline destructive in a card body (e.g., tunnel "Delete") | `--color-danger-bg` background, `--color-danger-fg` text, no border. Hover: slightly darker. |
| `tertiary` / `link` | Inline row actions ("Make static", "Revoke") | No bg, no border, `--color-text-secondary` text, underline on hover. |

**Sizes**

| Size | Height | Padding-x | Font | Use |
|---|---|---|---|---|
| sm | 28px | 10px | 12px | Table inline, dense toolbars |
| md | 32px | 14px | 13px | Default |
| lg | 36px | 18px | 14px | Empty-state CTA, primary modal action |

**Rules**
- All buttons in admin contexts include an icon. Pure-text buttons are reserved for tertiary inline actions.
- Never place two filled-primary buttons side by side. If you need two prominent actions, one becomes secondary.
- Destructive and primary buttons must not be adjacent. Either separate them with a divider, or move the destructive one into a danger zone block (see [4.5](#45-destructive-actions)).
- Trailing arrow `↗` is reserved for actions that navigate away or open in a new context.

**Do**
```
[ ↓ Download backup ]   ← primary, filled green, with icon
[ ⟳ Check now      ]   ← secondary, outlined, with icon
[ ↻ Restart        ]   ← destructive, red outline, with icon
   Make static  ·  Revoke   ← tertiary inline, no border
```

**Don't**
- Use the brand green for status badges or non-action elements.
- Mix icon-bearing and icon-less buttons in the same row.
- Use bold text inside a button (button labels are 500 already).

### 3.2 Status badge

A non-interactive label that communicates state. Visually distinct from buttons by being shorter, borderless, and using a soft semantic background.

**Anatomy**: optional 12px icon, label. Height 22px. Sentence case, single word or short phrase.

**Variants** (all use `--radius-pill`, no border, 12px font, 500 weight, 0–9px horizontal padding)

| Variant | Background | Text | Use for |
|---|---|---|---|
| `success` | `--color-success-bg` | `--color-success-fg` | Active, Running, Up to date, Lease (active), Reserved (held) |
| `neutral` | `--color-neutral-bg` | `--color-neutral-fg` | Down, Idle, External, Unmanaged, Disabled |
| `warning` | `--color-warning-bg` | `--color-warning-fg` | Stale, Expired, Restart required |
| `danger` | `--color-danger-bg` | `--color-danger-fg` | Failed, Error, Quarantined |

**Rules**
- Badges are never tappable. If a state can be acted on, transform it into a button (see [4.4](#44-action-positioning) and the auto-update pattern).
- Sentence case. "Active", not "active" or "ACTIVE". The DHCP leases table currently uses lowercase "active" — fix to sentence case.
- Use a leading checkmark icon only for `success` badges that confirm a desirable state ("Up to date", "Running"). Don't use icons for routine row-level statuses ("Active", "Lease").
- Maximum two-word label. If you need a longer phrase, you're describing too much — split into a label + secondary text.

### 3.3 Tabs (segmented control)

Used for switching between mutually exclusive views of the same data (Managed/Discovered devices, Leases/Reservations).

**Anatomy**: pill-shaped container with `--color-bg-subtle` background. Active tab has `--color-bg-surface` background, primary text, and a subtle border. Inactive tabs are transparent with secondary text.

**Rules**
- Use only when there are 2–4 options. For more, use a dropdown or a sidebar.
- Place directly under the page title (h1), left-aligned, with no other content on that horizontal line. Page-level CTAs go on the title line, not the tab line.
- Sentence case labels. No icons inside tabs.
- The default tab is the first one. Show its content immediately on page load — don't render an empty state if a tab isn't selected.

### 3.4 Toggle

Used for binary settings ("Automatically install when available", "Enable DHCP").

**Anatomy**: 32×18px pill. Off: `--color-border-default` background. On: `--color-brand` background. White circle (14×14px) slides 12px on toggle. Label sits to the right, 13px, primary text.

**Rules**
- Place the toggle to the **left** of its label. Click target includes the entire row (label + toggle).
- Never use a toggle for a tri-state. Use a select.
- Toggle change should apply immediately (no save button). If the action needs confirmation, use a button instead.

### 3.5 Select

Used for picking one of N values where N ≥ 4 or where the options need to be scrollable.

**Anatomy**: 32px tall, `--radius-md`, `0.5px solid --color-border-default`, white background, dark chevron at 8px from right edge.

**Rules**
- Pair with a label sitting to its left or above. Do not use a placeholder as a label.
- For 2–3 options, prefer tabs or a toggle.

### 3.6 Card

The default surface for grouping related content.

**Anatomy**:
- Background: `--color-bg-surface`
- Border: `0.5px solid --color-border-subtle`
- Radius: `--radius-lg`
- Padding: `var(--gap-card-inner) var(--gap-card-inline)` (20px / 24px)

**Header pattern** — every card with a title has a `card-head` row:
- Left: h2 title (16px/500), optional subtitle (13px secondary)
- Right: status badge OR contextual primary action OR overflow menu — pick one

**Internal sections** are separated by a horizontal `0.5px` divider with `var(--gap-section)` spacing above and below.

**Rules**
- Never nest cards inside cards. If you need internal grouping, use the inset pattern: `--color-bg-subtle` block with `--radius-md` (used for danger zones, metric tiles).
- Cards stack vertically with `var(--gap-card-stack)` between them.
- Card width is 100% of its column. On wide pages with side-by-side cards (e.g. DHCP stats + config), use a grid with `gap: var(--gap-card-stack)`.

### 3.7 Table

Used for listing many records with the same shape (Devices, DHCP leases, Reservations).

**Anatomy**:
- No outer card wrapper for the table itself; the table fills the page width
- Header row: 13px secondary text, weight 400 (not bold), `--color-bg-subtle` background, height 40px
- Data rows: 14px primary text, height 56px (allows two-line cells like device name + MAC)
- Row separator: `0.5px solid --color-border-subtle`
- Hover: row background becomes `--color-bg-subtle`
- Sticky header when content scrolls

**Cell types**

| Cell type | Treatment |
|---|---|
| Identifier (device name) | 14px primary, with 13px tertiary subtitle below (MAC, hash) |
| Monospace value (IP, MAC) | `var(--font-mono)`, 13px |
| Type / category | Soft pill in `--color-neutral-bg` |
| Status | Status badge (see [3.2](#32-status-badge)) |
| Timestamp | 13px secondary, relative ("just now", "11h ago", "17d ago") |
| Row actions | Right-aligned, tertiary buttons separated by 16px |

**Row actions** (e.g., "Make static", "Revoke"):
- Tertiary style, never primary or destructive-styled
- Right-aligned in their own column
- Destructive verbs ("Revoke", "Delete") get `--color-danger-fg` color but no background or border
- Hover: underline

**Rules**
- Sort indicators on column headers when sortable. Default sort is most-recent-first for time columns.
- Never put a primary button inside a row. Row actions are always tertiary inline.
- Empty cells use an em-dash `—`, never blank.
- Long values truncate with ellipsis and reveal full value on hover (title attribute).

### 3.8 Alert / callout

Used for inline cautionary or informational messages within a card body.

**Anatomy**:
- Background: semantic `*-bg`
- No border (the soft bg is enough)
- Padding: 10px 12px
- Radius: `--radius-md`
- Leading icon (14px), then message text (13px) in semantic `*-fg` color

**Variants**: `info`, `success`, `warning`, `danger` — use sparingly. Most pages should have zero or one alert.

**Rules**
- Use `warning` for irrecoverable risks ("Keep the passphrase somewhere safe — we can't recover it for you").
- Use `danger` for destructive confirmations or errors after a user action.
- Never use alerts for general help text. Help text is just secondary paragraph copy.

### 3.9 Stat tile

Used for displaying a single number or value prominently (Active Leases: 16, Pool Size: 201, system metrics).

**Anatomy**:
- Background: `--color-bg-subtle`, no border
- Radius: `--radius-md`
- Padding: 12px 14px
- Label on top: 12px secondary
- Value below: 17–24px primary, weight 500
- Optional unit suffix in 13px secondary

**Layout**: Use `display: grid; grid-template-columns: repeat(auto-fit, minmax(110px, 1fr)); gap: 8px;`.

**Rules**
- Stats are read-only. If you need an action related to a stat (e.g., "Edit pool"), put it elsewhere on the card.
- Never mix stat tiles and free-flowing label/value pairs in the same card. Pick one.

### 3.10 Progress bar

Used for utilization indicators (DHCP pool usage).

**Anatomy**: 4px tall track in `--color-bg-subtle`, fill in `--color-brand`. Label above showing "X / Y" with optional percentage.

**Rules**
- Switch fill color to `--color-warning-fg` above 80% utilization, `--color-danger-fg` above 95%.

---

## 4. Patterns

### 4.1 Page header

Every page has the same header structure.

```
┌───────────────────────────────────────────────┐
│  Page title                  [+ Primary CTA]  │  ← h1 left, single primary action right
│  Optional subtitle / description              │  ← 13px secondary, only if it adds context
│                                               │
│  [ Tab 1 ] [ Tab 2 ]                          │  ← optional segmented tabs
└───────────────────────────────────────────────┘
```

**Rules**
- h1 is 24px/500, sentence case.
- Page-level primary CTA (e.g., "Add tunnel") goes top-right on the title line, never below.
- Subtitle is optional. Settings doesn't need one. Devices doesn't need one. Use only when context isn't obvious.
- Tabs sit under the title line, left-aligned.

### 4.2 Empty states

Two distinct patterns, used in different situations. **The choice depends on whether the data is system-populated or user-populated.**

#### 4.2.1 Auto-populated empty (skeleton animation)

Use when data flows in automatically without user action: discovered devices, DHCP leases, traffic logs, anything observed by the daemon.

**Pattern**: Render 3–5 skeleton rows that match the real row dimensions, with a shimmer animation. The user understands data will appear.

**Skeleton anatomy**:
- Same height as a real row (56px for tables, ~80px for cards)
- Bars representing each cell, in `--color-bg-subtle`
- Shimmer animation: a 200% wide gradient sweeps left-to-right every 1400ms

```css
.skeleton {
  background: linear-gradient(
    90deg,
    var(--color-bg-subtle) 0%,
    var(--color-bg-page) 50%,
    var(--color-bg-subtle) 100%
  );
  background-size: 200% 100%;
  animation: shimmer var(--motion-skeleton);
  border-radius: var(--radius-sm);
}
@keyframes shimmer {
  0%   { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}
```

**When to stop showing skeletons**: After data arrives, OR after a soft timeout (10s). If 10s passes with no data and the source is genuinely empty (e.g., no devices have been discovered yet), fall through to a minimal "Waiting for data" message — but still no illustration, no CTA, since there is no user action to take.

**Examples in this product**: Devices → Discovered, DHCP → Leases (before first lease), Traffic logs.

#### 4.2.2 Manually-populated empty (illustration + CTA)

Use when the user must take action to create data: tunnels, reservations, custom rules, saved queries, anything user-authored.

**Pattern**: Centered illustration + headline + helper text + primary CTA.

**Anatomy**:
- Container: full card body, `min-height: 280px`, centered content
- Illustration: 96–128px, monochrome line art using `--color-text-tertiary`, no decorative color
- Headline: 16px/500, primary color. Describes the empty state plainly. "No tunnels yet."
- Helper: 13px secondary, max 2 lines, max ~140 characters. Explains what tunnels are or why you'd want one.
- Primary CTA: large button (36px), filled brand-green, with leading `+` icon. Same label as the page-level CTA — "Add tunnel".

```
            ┌─────────────────────┐
            │                     │
            │      [icon]         │   ← 96–128px line illustration
            │                     │
            │                     │
            │   No tunnels yet    │   ← 16/500 primary
            │                     │
            │  Tunnels route your │   ← 13/secondary, 1–2 lines
            │  traffic through    │
            │  a remote endpoint. │
            │                     │
            │  [ + Add tunnel  ]  │   ← lg primary button
            │                     │
            └─────────────────────┘
```

**Rules**
- One CTA only. If there's a secondary action like "Learn more", use a tertiary text link below the primary button.
- Never use a stock-photo illustration. Line art only, monochrome, drawn in the same style as the icon set.
- Don't reuse the same illustration across pages. Each empty state has a contextual icon (a tunnel for tunnels, a bookmark for reservations, etc.).
- Help text answers "what is this and why would I want one?" — not "click the button below to create one".

#### 4.2.3 Filtered empty (no results)

When a list is non-empty in absolute terms but the current filter/search returns nothing.

**Pattern**: Compact centered message, no illustration. Headline "No results", helper "Try adjusting your filters", optional "Clear filters" tertiary button. ~120px tall. Distinguishable from the two patterns above by its smaller footprint.

### 4.3 Loading states

Distinct from empty states — loading means "we're fetching, expect data".

| Situation | Treatment |
|---|---|
| Initial page load | Skeleton rows or skeleton cards matching the final layout |
| Refresh of existing data | No skeleton — keep showing stale data with a subtle spinner near the title |
| Mutation (e.g., creating a tunnel) | Disable the button, show inline spinner inside it, label changes to gerund ("Creating…") |
| Deletion | Optimistic — remove the row immediately, show toast on failure |
| Long-running ops (>2s) | Progress bar or step indicator, not a spinner |

**Rules**
- Never show a full-page spinner. Skeleton or partial loading.
- Spinners are 14–16px, `--color-brand` color, only as a button affordance or near a section title — never standalone.
- After 30s with no response, show a `danger` alert with retry option.

### 4.4 Action positioning

The location of an action signals its scope.

| Scope | Location | Style |
|---|---|---|
| Page-level | Top-right of page header | Primary, filled |
| Card-level (single action) | Bottom-right of card OR top-right of card head | Primary, filled — or secondary if it's not the main thing |
| Action on a setting row | Right end of the row | Sized to match the row (sm or md) |
| Row-level inside a table | Right end of the row, in a dedicated actions column | Tertiary inline |
| Destructive | Bottom of card, in a danger-zone sub-block | Destructive variant |
| Confirmation / next step | Bottom-right of modal | Primary; cancel sits to its left as secondary |

**State-as-button transformation** — when a status is actionable, the badge slot becomes a button:

```
Idle:    [ Auto-update           ✓ Up to date    ]   ← badge
Update:  [ Auto-update    [ ↓ Install update  ]  ]   ← primary button
Active:  [ Auto-update    [ ⟳ Installing…     ]  ]   ← primary, disabled, with spinner
```

This pattern applies anywhere state can become actionable: stale leases ("Renew"), inactive tunnels ("Reconnect"), pending updates, etc.

### 4.5 Destructive actions

Destructive = anything that overwrites, deletes, restarts, or disrupts state.

**In a card with safe actions**: separate the destructive action into a "danger zone" block at the bottom of the card.

```
┌─ Backup & restore ────────────────────────────-─┐
│  [explanatory text]                             │
│  [warning alert]                                │
│                                                 │
│  [ ↓ Download backup ]   ← safe primary         │
│                                                 │
│  ┌─ Inset --color-bg-subtle ────────────────-─┐ │
│  │  Restore from backup                       │ │
│  │  Overwrites your current config. Cannot    │ │
│  │  be undone.                                │ │
│  │                       [ ↑ Restore… ]       │ │
│  └────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
```

**Trailing ellipsis** — destructive verbs that open a confirmation dialog use a trailing `…` ("Restore…", "Delete…"). Verbs that fire immediately don't ("Restart" runs immediately because it's reversible — the daemon comes back).

**Confirmation dialog**:
- Required for destructive actions that overwrite or delete data
- Headline restates the action ("Restore from backup?")
- Body explains the consequence in one sentence
- For very dangerous ops (delete, restore), require typing the resource name to confirm
- Primary button is destructive variant, label is the verb ("Restore", "Delete")
- Cancel sits to the left as secondary

### 4.6 Stat clusters

When showing multiple metrics together (System info card, DHCP overview).

**Rules**
- 3–6 stats per cluster. More than 6 → split into multiple cards or use a chart.
- All stats in a cluster use the same tile size. Don't make one bigger because it "feels more important".
- Pair related stats: usage + capacity ("Active leases 16" / "Pool size 201") sit adjacent, not on opposite ends.

---

## 5. Page templates

Four canonical page archetypes. New pages should fit one of these.

### 5.1 Card stack (Settings)

Vertical stack of cards, no top-level CTA. Used for grouped configuration sections.

```
[Page title]
[Card: System]
[Card: Auto-update]
[Card: Backup & restore]
[Card: Account]
```

**Rules**
- Cards are full width.
- Each card represents one logical area. Don't lump unrelated settings into one card.
- Order cards from most-used to least-used, with deprecated/coming-soon at the bottom.

### 5.2 Table page (Devices, Reservations)

Page title + optional tabs + full-width table.

```
[Page title]
[Tabs: Managed / Discovered]
[Table]
```

**Rules**
- No card wrapper around the table. The table fills the page horizontally.
- If the table needs context (e.g., a filter bar, search), it sits between the tabs and the table.
- Empty state replaces the table area, not the whole page.

### 5.3 Card grid (Tunnels)

Page title + top-right CTA + grid of cards, one per item.

```
[Page title]                              [+ Add tunnel]
[Card][Card]
[Card]
```

**Rules**
- Grid: `repeat(auto-fit, minmax(360px, 1fr))`, gap `var(--gap-card-stack)`.
- Each card is a self-contained record with its own header (icon + title + status), body (key/value pairs), and footer (single destructive action).
- If a record has many actions (>2), use an overflow menu (`⋯`) instead of inline buttons.

### 5.4 Stats + table (DHCP)

Page title + stat cards row + tabs + table.

```
[Page title]
[Stats card]              [Config card]
[Tabs: Leases / Reservations]
[Table]
```

**Rules**
- Top row uses a 2- or 3-column grid for stats and config.
- The `Edit` button on a config card sits in the card head (top-right), not bottom — these cards are read-mostly with occasional edits.
- Tabs and table follow the [table page](#52-table-page-devices-reservations) rules.

---

## 6. Content & microcopy

### 6.1 Capitalization

- **Sentence case** for every UI string: page titles, card titles, buttons, table headers, status badges, tabs, dropdown options.
- **Code-like values** preserve their original casing: hostnames (`Galaxy S21 Leilah`), interface names (`wg_ward0`), MAC addresses, version strings.
- Specifically fix the existing inconsistencies:
  - DHCP leases column uses lowercase "active" → change to "Active"
  - "Auto-update" page header is fine (sentence case for hyphenated terms is unchanged)
  - "Make Static" → "Make static"

### 6.2 Action verbs

Buttons use the imperative verb that describes the action.

| Good | Bad | Why |
|---|---|---|
| Restart | Click to restart | The button is the click target |
| Download backup | Get backup file | Specific verb, specific noun |
| Restore… | Restore from backup | Trailing ellipsis indicates confirmation step |
| Add tunnel | New | Verbs scale better when there are many object types |
| Make static | Convert to static reservation | Shorter is better when context is obvious |

### 6.3 Status labels

Single word preferred. Sentence case. No icons unless it's a confirming success.

| Use | Avoid |
|---|---|
| Active | ACTIVE, active, Online |
| Down | down, Offline, Disconnected |
| Idle | IDLE, waiting, paused |
| Up to date | up-to-date, Latest, Current |

### 6.4 Empty state copy

- Headline: "No [things] yet." Period, sentence case, optional "yet" if user action will populate it.
- Helper: 1–2 sentences explaining what the thing is and why someone would create one. Avoid "Click below to add one" — that's what the button is for.

**Examples**

| Page | Headline | Helper |
|---|---|---|
| Tunnels (manual empty) | No tunnels yet. | Tunnels route your traffic through a remote VPN endpoint. |
| Reservations (manual empty) | No reservations yet. | Reservations pin a device to a fixed IP, even after its lease expires. |
| Devices Discovered (auto empty) | _(skeleton rows, no copy)_ | _(after timeout)_ Waiting for devices to appear on the network. |

### 6.5 Microcopy tone

- Direct. No "please", no "kindly", no marketing voice.
- Confident. State what something does, not what it might do. "Restart will disconnect active sessions" not "Restart may disconnect sessions".
- Explain consequences for destructive actions in one sentence. "Cannot be undone" is a complete justification.
- Avoid jargon when the user might not be technical. Where jargon is unavoidable (WireGuard, DHCP), use it without apology — your users know what these mean.

---

## 7. Iconography

### 7.1 Style

- **Source**: Lucide (or equivalent stroke-style set). Stroke width 2px. Round line caps and joins.
- **Sizing**:
  - 14px inside buttons (default)
  - 12px inside badges
  - 16px inline with body text or as decorative anchor in card heads
  - 24–32px in empty-state illustrations (composed of multiple primitives)
- **Color**: inherits from `currentColor`. Never set fill or stroke directly except in illustrations.

### 7.2 Specific icons (canonical mapping)

| Concept | Icon |
|---|---|
| Restart, retry | `rotate-ccw` |
| Check for updates | `refresh-cw` |
| Download | `download` (tray + down arrow) |
| Upload, restore | `upload` (tray + up arrow) |
| Delete, revoke | `trash-2` |
| Confirm, success | `check` |
| Warning | `alert-triangle` |
| Add | `plus` |
| Edit | `pencil` |
| Settings (gear pages) | `settings` |
| Devices | `smartphone`, `laptop`, `router`, `tv`, `monitor` (use the device-type-specific one in the device row) |
| Tunnel | `shield` or `globe` (whichever is established in the existing UI) |

**Rules**
- One icon per button. Never two.
- Icons in tables (left of device name) reflect the device type; pick from a fixed enum.
- Don't invent new icons for established concepts. If something doesn't exist in the canonical mapping, propose an addition rather than ad-hocing.

---

## 8. Accessibility

### 8.1 Color contrast

- Text on `--color-bg-surface` (white) must meet WCAG AA: secondary text 4.5:1, primary text 7:1.
- Text on semantic backgrounds (`*-bg`) uses the matching `*-fg` token, which is pre-tested for AA.
- Never rely on color alone to convey state. Status badges include a label; never use just a colored dot.

### 8.2 Keyboard

- Every interactive element is reachable by Tab, in document order.
- Focus ring: `0 0 0 2px --color-brand` outside the element, with 2px offset. Never remove it without replacement.
- Modal traps focus until dismissed. Escape closes.
- Tables: arrow keys navigate cells, Enter activates the focused row's primary action.

### 8.3 Screen reader

- All icon-only buttons have an `aria-label`.
- Status badges use `role="status"` and `aria-label="Status: Active"`.
- Tables use proper `<thead>`, `<th scope="col">`, `<th scope="row">` for identifier columns.
- Loading states announce via `aria-live="polite"`.

### 8.4 Motion

- Respect `prefers-reduced-motion: reduce` — skeleton shimmer halts to a static state, transitions become instant.

---

## Appendix A — Checklist for new pages

Before merging a new admin page or refactoring an existing one, verify:

- [ ] Page header uses the canonical structure (h1 + optional CTA top-right + optional tabs)
- [ ] Page fits one of the four archetypes ([5.1](#51-card-stack-settings)–[5.4](#54-stats--table-dhcp))
- [ ] All buttons use a defined variant; primary appears at most once per surface
- [ ] All status badges use `success` / `neutral` / `warning` / `danger` tokens; no raw colors
- [ ] Destructive actions are isolated from primary actions
- [ ] Empty state matches the auto-fill vs manual-fill distinction
- [ ] All strings are sentence case
- [ ] All icons come from the canonical Lucide set; sizes match per-context guidelines
- [ ] All interactive elements reachable by keyboard, with visible focus rings
- [ ] No raw hex values in CSS — only tokens