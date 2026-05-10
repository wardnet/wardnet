# Wardnet Forge Migration — Progress & Notes

> Multi-session, single PR. This doc lives at the repo root for the duration of the
> rework and gets deleted in the final commit before merge.

Branch: `chore/design-system`
Started: 2026-05-08
Forge reference lives under `design-system/` (mocks + tokens + Tailwind config) and
in `.agents/skills/using-design-system/SKILL.md`.

---

## Strategy (decided)

1. **Forge as source of truth.** Adopt `design-system/styles.css` tokens verbatim
   into `source/admin-app/web/src/index.css` and `source/marketing-site/src/index.css`. Both
   surfaces (admin web-ui and public site) end up on the same token set so
   "matches the admin web-ui" stops being a comment in CSS and starts being
   a fact.
2. **Tailwind 4 + Forge classes, both.** Forge ships a Tailwind config that maps
   every token to a utility (see `design-system/tailwind.config.js`). We use
   Forge component classes (`.card`, `.stat`, `.pill`, `.btn`) where Forge has
   them and Tailwind utilities (`bg-card`, `text-ink`, `rounded-lg`, `shadow-card`)
   where it doesn't. Both compile against the same CSS variables, so theme
   toggles flip everything in lockstep.
3. **Radix-direct, not shadcn.** Rip out the shadcn wrappers in
   `components/core/ui/` and replace them with thin React components that style
   Radix primitives with Forge classes. shadcn's value is Tailwind opinions and
   token decisions we're throwing away anyway; what we want is Radix's behavior
   (focus traps, keyboard nav, ARIA, portal logic). `radix-ui@1.4.3` is already
   installed as a unified package.
4. **Tailwind 4.2.4** is the latest as of 2026-05-08 — already pinned, no bump.
5. **Public site = Ward Navy chrome.** `source/marketing-site/` retires its
   `--brand-indigo|slate|green` tokens and adopts Forge tokens, with the same
   Ward Navy chrome (`--side-bg`) for navbar/footer that the admin sidebar uses.
   Marketing visitor and admin user see the same Wardnet — heavier than a
   typical landing page, but consistent and on-brand.
6. **Single PR, many sessions.** Each session takes a vertical slice (a
   primitive, a compound component, a page) and lands a commit. Doc gets updated
   in the same commit so the checklist always reflects HEAD.

### Locked defaults (from open-questions pass on 2026-05-08)

| Decision               | Outcome                                                          |
| ---------------------- | ---------------------------------------------------------------- |
| Density                | **Bake comfortable**, no toggle. `--pad: 18px`, `--row-h: 52px`. The `[data-density="compact"]` block is dropped from `styles.css` when we vendor Forge — not just unused, removed. |
| Sidebar style          | **Floating** is the default and the only variant we ship. Solid-dark + light variants from Forge are reference-only. |
| Tweaks panel           | **Skip entirely.** No tweaks panel, no density UI, no sidebar style picker. The only live preference is light/dark theme — wired in the topbar (or wherever it currently lives). |
| Public site chrome     | **Match admin.** Ward Navy navbar + footer, --bg surface for content, Signal Green CTAs. |
| Icons                  | **Keep `lucide-react`.** Forge `<Icon>` set in `primitives.jsx` is reference-only. Adopt lucide's `strokeWidth={1.7}` default to match Forge's hand-tuned strokes. |
| CVA                    | **Keep `class-variance-authority`** for our own primitives (Button, Pill, Card variants). |
| `tw-animate-css`       | **Removed (slice 16).** Originally retained for dialog/sheet/dropdown entrance animations. Modal/AlertModal/Popover/DropdownMenu/Select all landed Forge keyframes; Sheet was the last consumer of tw-animate-css utilities and was retired in slice 16 (replaced by Drawer + inline-edit pattern). Package dropped from `admin-app/web/package.json`; `@import` removed from `index.css`. |
| Accent color           | **Lock to Signal Green.** No user picker. `--accent` is the only brand hue; warn/danger/info still vary per status. |

### How this evolved

The first plan was "Forge CSS verbatim — drop shadcn, drop Tailwind, hand-roll
everything." Then Forge added `tailwind.config.js`, so Tailwind utilities are
back in scope. Then we noticed `radix-ui` is already a dep, so dropping shadcn
doesn't mean rebuilding behavior. Final shape: tokens + classes from Forge,
behavior from Radix, utilities from Tailwind.

---

## Stack changes

| Drop                              | Keep                                              | Add                                       |
| --------------------------------- | ------------------------------------------------- | ----------------------------------------- |
| `shadcn` (CLI / generator)        | `radix-ui` 1.4.3                                  | `@fontsource-variable/inter-tight`        |
| `@fontsource-variable/geist`      | `tailwindcss` 4.2.4 + `@tailwindcss/vite` 4.2.4   | `@fontsource-variable/jetbrains-mono`     |
| shadcn wrappers in `core/ui/`     | `lucide-react` (with default `strokeWidth={1.7}`) | Forge `styles.css` (vendored)             |
|                                   | `cmdk`, `sonner`                                  | Forge tokens in `index.css` `@theme`      |
|                                   | `class-variance-authority`, `clsx`, `tailwind-merge` |                                       |
|                                   | `@tanstack/react-table` (powers `.tbl`)           |                                           |
|                                   | `next-themes` (configured with `attribute="data-theme"`) |                                    |
|                                   | `recharts` (line/area charts §10)                 |                                           |

---

## Token migration (admin + site)

Replace shadcn-style tokens (`--background`, `--foreground`, `--primary`, `--muted`,
`--accent`, `--destructive`, `--border`, `--ring`, `--card`, `--popover`,
`--secondary`, `--sidebar*`, `--chart-N`, `--radius*`) with Forge:

- Surfaces: `--bg`, `--bg-elev`, `--bg-sunken`, `--bg-card`, `--line`, `--line-strong`
- Ink: `--ink`, `--ink-2`, `--ink-3`, `--ink-4`
- Sidebar: `--side-bg`, `--side-line`, `--side-ink`, `--side-ink-2`, `--side-ink-active`, `--side-active-bg`
- Brand/status: `--accent` (+ `-ink`, `-soft`, `-soft-ink`), `--warn`, `--danger`, `--info` (each with soft variants)
- Radius: `--radius-sm` 6 / `--radius` 10 / `--radius-lg` 14 / `--radius-xl` 20
- Elevation: `--shadow-card`, `--shadow-pop`
- Density: `--pad: 18px`, `--row-h: 52px` (comfortable only — no compact block)
- Type: `--font-sans` Inter Tight, `--font-mono` JetBrains Mono

Theme switch: move `next-themes` from class strategy (`.dark`) to attribute
strategy (`[data-theme="dark"]`) so Forge's existing token block fires correctly.

Tailwind 4 `@theme inline` block in `index.css` keeps utilities (`bg-card`,
`text-ink`, etc.) backed by these CSS vars. Mirror Forge's `tailwind.config.js`
mappings — Tailwind 4 reads them out of `@theme`, no separate config file needed.

---

## Voice & content rules (apply everywhere)

| Do                                                                          | Don't                                                          |
| --------------------------------------------------------------------------- | -------------------------------------------------------------- |
| State + number + window: *"Filter is on. 4,747 of 1.25M queries blocked today."* | Vague reassurance: *"DNS filtering is currently active."* |
| Name the subject: *"Revoke lease for Galaxy S24 Pedro?"*                    | Pronoun-only: *"Are you sure you want to remove this device?"* |
| Verb-first actions: *"Bring up United States"*                              | Padded actions: *"Connect to United States now"*               |
| Mono for facts (IPs, MACs, ports, durations, hashes)                        | Mono for prose                                                 |
| Action lives on the right; Cancel left, primary right, destructive last & red | Centered button rows, primary on the left                    |

Banned words: **Currently**, **your**, **we'll**. No emoji except country flags
in tunnel context.

Six principles (gate every screen before marking it done):
1. Numbers earn the room.
2. Status before chrome.
3. Mono for facts.
4. Surfaces, not pages — the shell never moves.
5. Dense without crowded — comfortable default; compact mode optional.
6. Action lives on the right.

---

## Primitives — `source/admin-app/web/src/components/core/ui/`

Replace each shadcn wrapper with a Radix-backed Forge component. `n/a` = no
Radix primitive needed (pure visual or HTML element).

| File              | Radix              | Forge surface                                          | Status |
| ----------------- | ------------------ | ------------------------------------------------------ | ------ |
| button.tsx        | n/a                | `.btn` / `.btn--primary` / `.btn--ghost` / `.btn--danger` / `.btn--sm` | [x] |
| card.tsx          | n/a                | `.card` / `.card--flush` + `.card__head` + `.card__foot` (added) | [x] |
| badge.tsx → pill  | n/a                | `.pill` / `.pill--ok|warn|down|info|ghost` (renamed `Badge` → `Pill`) | [x] |
| dialog.tsx → modal | Dialog            | `.modal` (`.scrim` / `.modal__head` / `.modal__body` / `.modal__foot`) (renamed `Dialog` → `Modal`) | [x] |
| alert-dialog.tsx → alert-modal | AlertDialog | `.modal` (no new modifier — danger framing is button-level; renamed `AlertDialog` → `AlertModal`) | [x] |
| sheet.tsx         | —                  | dropped — replaced by inline detail/edit pattern (form consumers) and Drawer (mobile-nav consumer); deleted in slice 16 | [-]    |
| dropdown-menu.tsx | DropdownMenu       | `.popover` surface + `.menu-item` / `.menu-separator` (added) | [x] |
| popover.tsx       | Popover            | `.popover` (added — `--bg-card` + `--shadow-pop` + side-aware entrance) | [x] |
| select.tsx        | Select             | `.select-trigger` (added) + `.popover` `.select-content` (added) + `.menu-item` (incl. `[data-state="checked"]::after` checkmark, added) | [x] |
| switch.tsx → toggle | Switch           | `.toggle` (renamed `Switch` → `Toggle`)                | [x]    |
| tabs.tsx          | Tabs               | `.tabs` (pill) for view switches — only surface this slice; legacy `variant="line"` had zero call sites and was dropped. Underline page-nav surface (`.tabs-bar`) deferred until a router-driven nav consumer needs it. | [x] |
| radio-group.tsx   | —                  | dropped — sole consumer (`MyDevice`'s direct/VPN choice) refactored to reuse the existing `RoutingSelector` compound; see Findings | [-] |
| label.tsx         | Label              | `.label` (added — standalone) + existing `.field label` (descendant) — share rules via comma selector | [x] |
| input.tsx         | Input              | `.input` (added — standalone) + existing `.field input` — share rules via comma selector | [x] |
| textarea.tsx      | Textarea           | `.textarea` (added — standalone, with `min-height` + `field-sizing: content`) + existing `.field textarea` — share rules | [x] |
| input-group.tsx   | —                  | dropped — sole consumer was `core/ui/command.tsx`; the new `Combobox` composite renders its search input via a `.combobox-input` Forge class, so InputGroup has zero consumers | [-] |
| ipv4-input.tsx    | n/a (stays in `core/ui/`) | wrapper carries `.input` + `data-segmented`; nested `<input>`s sit transparent inline; `.input[data-segmented]` block in Forge overrides padding/font-family/width and the `.field input` cascade for inner inputs; `.input:focus-within` extension on the existing focus rule for wrapper-style focus ring | [x] |
| mac-input.tsx     | n/a (stays in `core/ui/`) | same `.input` + `data-segmented` template as Ipv4Input — six hex segments instead of four octets | [x] |
| command.tsx       | Combobox           | renamed Command → Combobox (cmdk used only as a filterable select in this codebase, not a command palette); high-level composite owning Popover + trigger Button + search + list scaffold; `.combobox-trigger` / `.combobox-input` / `.combobox-list` / `.combobox-empty` Forge classes added; items reuse `.menu-item` (cmdk `data-selected` bridged to `[data-highlighted]` via comma selector) | [x] |
| chart.tsx         | (recharts)         | Forge §10 chart rules — `.chart` (added — hairline horizontal grid only, mono Y-axis ticks, accent-soft brush, `.recharts-default-tooltip` restyled in-place) + `.tooltip` (added — `--bg-card` + `--shadow-pop` card surface, mono numerics) + chart palette `--chart-1..4` (accent → info → warn → ink-3 per §10 rule 01); cssVars per-instance bridge kept for `<Line stroke="var(--color-key)">` plumbing — comment rewritten | [x] |
| data-table.tsx    | (tanstack/table)   | Forge §05 data-table — `.tbl` (existing rules anchored to spec, `tr[data-clickable]` row affordance + `.tbl--fixed` modifier for `fixedLayout` API + `.tbl-empty` cell + `.tbl-wrap` non-clipping card-flush surface added so sticky <th> still promotes to the page scroll) + `.host` (unchanged, owned by row-cell renderers in T3-α); primitive drops shadcn `<TableRow>` / utility classes, renders <thead>/<tbody>/<tr>/<th>/<td> directly under `.tbl-wrap`, public API (`columns`, `data`, `emptyMessage`, `onRowClick`, `fixedLayout`) preserved so compound consumers keep compiling — ported data-table.tsx + .tbl/.host rules in styles.css on 2026-05-10 | [x] |
| toaster.tsx       | (sonner)           | Forge §14 toast — `.toast` extended (variants `--ok` / `--warn` / `--down` / `--info` mirror `.pill--*` tone vocabulary, `--bg-card` + `--shadow-pop` card surface, 1px `--line` border tinted per tone, mono numerics opt-in, existing `pop` entrance animation kept) + Sonner runtime bridge (`[data-sonner-toaster]` sets `--normal-bg/--normal-text/--normal-border/--border-radius` to Forge tokens, `[data-sonner-toast][data-type="success"\|"warning"\|"error"\|"info"]` paint borders + icon hue, `[data-button]` / `[data-cancel]` / `[data-close-button]` restyled to `--ink` / `--bg-sunken` / `--bg-card` so consumers don't wire per-toast classes); wrapper drops the broken `var(--border)` + shadcn green/red `!bg-*` overrides and forwards `toast` / `toast--ok` / `toast--warn` / `toast--down` / `toast--info` via `toastOptions.classNames` so Forge-class authors and Sonner-data-attr authors land on the same selectors; public API (`Toaster` named export, `ToasterProps`) preserved — ported toaster.tsx + `.toast` rules in styles.css on 2026-05-10 | [x] |

### New primitives to introduce (don't exist yet)

| Primitive   | Source                              | Status |
| ----------- | ----------------------------------- | ------ |
| StatTile    | `design-system/primitives.jsx` — added `StatTile` primitive in `forge-web/src/primitives/stat-tile.tsx` (slot-based: `label` / `value` / `unit` / `sub` / `bar` / `spark` / `pill`); studio's BEM `.stat` block was already in `styles.css` so this slice only consumes it — `spark` accepts a `ReactNode` to keep StatTile decoupled from the Sparkline primitive (slice 2b); per-primitive `@wardnet/forge-web/stat-tile` export wired in `package.json` on 2026-05-10 | [x]    |
| Sparkline   | `design-system/primitives.jsx` — added `Sparkline` primitive in `forge-web/src/primitives/sparkline.tsx` (Option A inline-SVG: single `<polyline>` over a 100×40 viewBox + optional 0.12-opacity area wash, `preserveAspectRatio="none"` so the host box owns aspect); `--spark-color` CSS-var hook in `.sparkline` defaults to `--accent` and is per-instance themable via `style={{ "--spark-color": "var(--info)" }}`; new `.sparkline` / `.sparkline__line` / `.sparkline__area` rules in `styles.css` mirror §10 throughput (1.5 hairline, `non-scaling-stroke`) and §13 area-wash precedent; per-primitive `@wardnet/forge-web/sparkline` export wired in `package.json` on 2026-05-10 | [x]    |
| Donut       | `design-system/primitives.jsx` — **deferred 2026-05-10**: no consumer in `source/admin-app/web/` or `source/marketing-site/`; reintroduce only when a feature actually needs a ring chart | deferred |
| Icon set    | `design-system/primitives.jsx` — **CSS-only adoption 2026-05-10**: kept `lucide-react`, added `svg.lucide[stroke-width="2"] { stroke-width: 1.7 }` to `styles.css` so the 1.7 Forge convention applies globally without overriding consumer-side per-instance overrides (`strokeWidth={1.2|1.5|…}` emit different attribute values and fall outside the selector); 24 lucide-react import sites in `admin-app/web/src` pick this up automatically; Forge's `<Icon>` set in `primitives.jsx` remains reference-only (no `forge-web/src/primitives/icon.tsx` shipped) | [x] |
| Field       | composition (Label + control + help + edit/read swap) on top of `.field` — added with the form-row slice | [x] |
| Drawer      | Radix Dialog wrapped with `.scrim` + new `.drawer` class (edge-pinned slide-in panel; left/right `data-side` variants) — added with the MobileMenu slice | [x] |

---

## Compound components — `source/admin-app/web/src/components/compound/`

Each is a Forge-native rewrite. Move utility-class soup → Forge classes; mono-wrap
all facts; status pills via `.pill--*`.

| Component                       | Status | Notes                                                      |
| ------------------------------- | ------ | ---------------------------------------------------------- |
| AllowlistTable                  | [x]    | `.tbl` via DataTable (slice 1b primitive) — domain cell stays mono fact, reason + added cells use `text-ink-3`, `fixedLayout` keeps the 12rem actions column pinned; dropped shadcn `text-sm` so `.tbl`'s 13px owns the row font-size; public API (`entries` / `onDelete` / `onAdd`) preserved on 2026-05-10 |
| ApiErrorAlert                   | [x]    | full-card error callout — switched ad-hoc `border-danger/30 bg-danger/5 text-danger` to Forge soft-tone tokens (`bg-danger-soft`, `border-danger-soft`, `text-danger-soft-ink`); `role="alert"` added; not a Banner (inline rounded callout, not full-width strip) on 2026-05-10 |
| BlocklistTable                  | [x]    | `.tbl` via DataTable (slice 1b primitive) — name cell stack uses `.col` with `min-w-0`, URL + entry-count cells use `.mono` per styles.css §05 contract, outer wrapper on `.col gap-16` + `.row justify-end`, `fixedLayout` pins auxiliary column widths; public API (`blocklists` / `onRefresh` / `onToggle` / `onEdit` / `onDelete` / `refreshingId` / `onAdd`) preserved on 2026-05-10 |
| ConfirmDialog                   | [x]    | AlertDialog-backed — thin wrapper over forge-web `AlertModal` (Radix `AlertDialog` + `.modal`/`.scrim` Forge classes); destructive/default Buttons via `asChild`; public API (`open`/`onOpenChange`/`title`/`description`/`confirmLabel`/`onConfirm`/`destructive`) preserved on 2026-05-10 |
| ConnectionBanner                | [x]    | thin top banner, mono ws status — now thin wrapper over `<Banner tone='down'>` (forge-web/banner primitive added on 2026-05-10) |
| ConnectionStatus                | [x]    | sidebar dot+label indicator — replaced raw `bg-emerald/yellow/red-400` with Forge `bg-accent/warn/danger` tokens on 2026-05-10; pill shape was the wrong fit (sidebar footer is dot+text, not a chip), kept the dot+label visual |
| CountryCombobox                 | [x]    | thin wrapper over `<Combobox>` from `@wardnet/forge-web/combobox` (cmdk inside Forge Popover, items reuse `.menu-item`); trigger row swapped from raw Tailwind `flex items-center gap-2` to Forge `.row .gap-8`; placeholder kept on `text-ink-3` token utility; public API (`countries` / `value` / `onChange` / `placeholder` / `disabled`) preserved on 2026-05-10 |
| CronSchedulePicker              | [x]    | field cluster — popover body's schedule rows (Repeat / Interval / Days / Day of month / At) ported from ad-hoc `<div className="flex flex-col gap-1.5"><p className="text-xs font-medium text-ink-3">…</p>` Field-mimics to real `<Field label=…>` primitives so `.field` (12px ink-3 label, 6px gap) owns the rhythm; outer `<Field label={label}>` trigger anchor kept; inner DOW map var renamed `label` → `dayLabel` to dodge prop shadow; summary chip stays on `bg-sunken/50` + `text-ink-3` Forge tokens; `{ value, onChange, label }` API preserved on 2026-05-10 |
| DashboardStatCard               | [x]    | -> StatTile primitive                                      |
| DashboardUsageBar               | [x]    | `.bar` — `<div class="bar"><span style="width:N%; background:var(--accent\|warn\|danger)"/></div>`; thresholds `>80% danger`, `>50% warn`, else `accent`; replaced shadcn `bg-sunken/bg-accent/bg-danger` and stray `bg-yellow-500` with Forge `.bar` + CSS-var fill on 2026-05-10 |
| DetailPageHeader                | [x]    | H1 + status pill + breadcrumb — title now uses `.h-title`, meta uses `.h-sub`, layout via `.row`/`.col` helpers; breadcrumb mirrors `.topbar__crumbs` ink-3 trail + ink current segment with lucide `ChevronRight` separator; replaced inline `text-2xl font-semibold tracking-tight` and `text-ink/70` with Forge tokens (T3-δ, 2026-05-10) |
| DeviceIcon                      | [x]    | lucide stays per locked decision; slice 2d sets 1.7 stroke as default — tagged inline `SetTopBox` SVG with `lucide` class so the global `svg.lucide[stroke-width="2"]` selector applies; `text-ink-3` token preserved; API unchanged |
| DeviceSelect                    | [x]    | Radix Select wrapper via `@wardnet/forge-web/select` — already conformant (slice 0a primitive emits `.select-trigger` / `.popover.select-content` / `.menu-item`); verified on 2026-05-10, no code changes |
| DeviceTable                     | [x]    | `.tbl` + `.host` row                                       |
| DhcpConfigCard                  | [x]    | edit-mode card protocol — folded `EditDhcpConfigSheet` into the card's edit mode, hook-coupled (`useUpdateDhcpConfig`) per the DeviceSettingsCard precedent |
| DhcpLeaseTable                  | [x]    | `.tbl`                                                     |
| DhcpReservationTable            | [x]    | `.tbl`                                                     |
| DhcpStatusCard                  | [x]    | first-card pattern (status pill + headline number) — replaced shadcn `Field`+Toggle body row with `CardAction` toggle, swapped `StatusBadge`+`DashboardUsageBar` for raw `Pill`+`.bar` and `stat__label`/`stat__value` headline numbers per the studio mock; kept `{ status, onToggle, isPending }` API on 2026-05-10 |
| DhcpSummaryCard                 | [x]    | StatTile-derived                                           |
| DiscoveryPlaceholder            | [x]    | `.empty` outer frame (dashed border + lg radius via Forge tokens); rich scan-bar / particles / breathing-rows animations preserved inside; `!p-0` overrides the class's 40px padding so the skeleton goes full-bleed |
| EmptyStatePlaceholder           | [x]    | `.empty` outer frame (dashed border + `--radius-lg`); concentric ripple rings + stacked-document + plus-badge preserved as the component's signature, retinted onto `--accent`/`--ink-3` via `color-mix` so no raw Tailwind palette refs remain (slice T3-γ on 2026-05-10) |
| FilterRuleTable                 | [x]    | `.tbl` (already conformant — verified post-slice-1b on 2026-05-10; no code changes required) |
| HostCell                        | [x]    | `.host` markup — wraps `icon` in `.avatar`, primary in `.name`, secondary in `.mac.mono`; preserves `{ primary, secondary, icon }` API (slice T3-δ on 2026-05-10) |
| JobProgressDescription          | [x]    | mono job state                                             |
| Logo                            | [x]    | inline SVG shield + signal mark; tinted from `--accent` (signal-green fill), `--side-bg` (signal arcs + ping), `--ink` (marketing-size hairline outline); single `viewBox="0 0 32 32"` path set with a 40px size threshold flipping chrome → marketing variant (extra outer arc + shield outline) so 24-28px Sidebar/AppLayout chrome and 60-80px AuthLayout marketing share one component; preserved `{size?, className?}` API; dropped `logo.png` import (slice T3-δ on 2026-05-10) |
| LogViewer                       | [x]    | `.logs` / `.logrow` + level filter (`is-warn`/`is-err`/`is-info`) — uses `.t`/`.l`/`.m` slot triplet from screens.jsx mock; connection dot uses `bg-accent`/`bg-danger` Forge tokens (was raw `bg-green-500`/`bg-red-500`); empty state rendered as a single `.logrow.is-info` for shell consistency on 2026-05-10 |
| MobileMenu                      | [x]    | hamburger → `<Drawer side="left">` hosting the existing `<Sidebar />` — slice 16 |
| PageHeader                      | [x]    | `.h-title` heading + `.row` layout with right-aligned actions — slice T3-δ on 2026-05-10; replaced `text-2xl font-bold tracking-tight` with Forge `.h-title` and swapped Tailwind flex utilities for `.row`/`gap-8`; API (`title`, `actions`) unchanged |
| ProfileToggleList               | [x]    | toggle-row list — already conformant: wraps `Pill` + `Toggle` Forge-web primitives; container/row utilities resolve through Forge tokens (`border-line`, `text-ink-3`, `rounded-md`); no raw hex / inline styles / non-token Tailwind palette refs; verified on 2026-05-10 (slice T3-δ), no code changes required |
| RecentErrorsCard                | [x]    | `.logs` / `.logrow` (`is-warn`/`is-err`/`is-info`) inside Card chrome — slice T3-γ on 2026-05-10; dropped Pill in favour of canonical `.t`/`.l`/`.m` triplet |
| RoutingSelector                 | [x]    | Forge `<Select>` — "Direct (no VPN)" plus one item per tunnel (flag + label); empty-tunnel state shows disabled select with admin/non-admin help text. Tokens (`text-ink-3`, `text-accent`) already conformant; no shadcn / hex. Slice T3-δ on 2026-05-10 — row originally read "radio-group field", but screens.jsx has no routing picker mock, so dropdown shape kept (decision (a)) and row flipped to reflect the actual implementation. |
| Sidebar                         | [x]    | Forge `.side` family throughout — `.side__brand` (Logo passed `className="logo"` so the existing CSS rule `.side__brand .logo` retints it) + brand text, `.side__nav` with `.side__item`/`.is-active` rows driven by `NavLink`'s active state, `.side__foot` containing `<UpdateBanner>` (admin-only), `<ConnectionStatus>` wrapped in `.side__status`, and `.side__links` for API-docs / sign-out / sign-in. All shadcn / Tailwind palette refs (`bg-side`, `text-side-ink/60`, `border-side-line`, `bg-side-active`, etc.) dropped in favour of the Forge classes. Floating chrome is baked into `.side` per slice 0a — no variant prop. Public API (`onNavigate?`) unchanged; AppLayout untouched. Slice T3-δ on 2026-05-10 |
| StatusBadge                     | [x]    | `.pill--*` via the `Pill` primitive — already conformant: tone vocabulary (`success`/`neutral`/`danger`) maps to Pill variants (`ok`/`ghost`/`down`); verified post-slice-1c on 2026-05-10, no code changes required |
| TunnelCard                      | [x]    | `.tcard` family — `.tcard` root replaces shadcn `Card`/`CardHeader`/`CardContent`, `.tcard__head` hosts the `.tcard__flag` tile + `.tcard__title`/`.tcard__sub` (country · provider · `iface.mono`) + StatusBadge with `.dot`, `.tcard__grid` is a `<dl>` with Endpoint and Last handshake `<dt>/<dd>` pairs, `.tcard__throughput` carries the `↑ tx · ↓ rx` mono readout strip; `Button` variant=`outline\|destructive` size=`sm` for Test/Delete; test result + error callouts use Forge `--line`/`--bg-sunken` and `--danger-soft`/`--danger-soft-ink` tokens; public API (`tunnel`, `providers`, `onDelete`) preserved on 2026-05-10 (slice T3-δ) |
| TunnelGrid                      | [x]    | responsive `grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4` wrapper around `TunnelCard`; loading state keeps Forge `Card`/`CardContent` chrome with `text-ink-3` token; empty state delegates to `EmptyStatePlaceholder` — slice T3-δ on 2026-05-10; bumped to 3-column at `lg`; API (`tunnels`, `providers`, `isLoading`, `isError`, `onDelete`, `onAdd`) unchanged |
| UncleanShutdownBanner           | [x]    | banner, danger-soft tones — now thin wrapper over `<Banner tone='down'>` with `actions={<Button>Dismiss</Button>}` (forge-web/banner primitive added on 2026-05-10) |
| UpdateBanner                    | [x]    | sidebar update-prompt link (NOT a banner — full-width Banner compound covers ConnectionBanner/UncleanShutdownBanner; this is a chip rendered inside the sidebar by `Sidebar.tsx`); already on Forge `accent` tokens (`bg-accent/10`, `text-accent`, `bg-accent/15` hover) — verified on 2026-05-10, no code changes required. File name kept for now; rename to `UpdatePrompt`/`UpdateChip` could land in T8 polish if churn is worth it. |

---

## Feature components — `source/admin-app/web/src/components/features/`

| Component                | Status | Notes                                            |
| ------------------------ | ------ | ------------------------------------------------ |
| BackupCard               | [x]    | edit-mode card protocol — folded ExportDialog/RestoreDialog Modals into the card's `mode: view\|export\|restore` body, Cancel/primary in `CardFooter`, triggers in `CardHeader`'s `CardAction` per the DhcpConfigCard precedent; preview's incompatible callout swapped from `border-danger/50 bg-danger/10` to `bg-danger-soft` + `text-danger-soft-ink` Forge tokens |
| CreateReservationInline  | [x]    | inline create card mirroring CreateTunnelInline; rendered above DhcpReservationTable (renamed from CreateReservationSheet, sheet dropped) |
| CreateTunnelInline       | [ ]    | inline form + WireGuard config paste             |
| DashboardLogWidget       | [ ]    | `.logs`                                          |
| DeviceDnsFilterCard      | [x]    | edit-mode card protocol per DhcpConfigCard / DeviceSettingsCard precedent — Card/CardHeader/CardAction (Edit) → CardContent/CardFooter (Cancel + Save) on `editing`, hook-coupled (`useUpdateDeviceFilterSettings`); read-mode reshaped to `<dl>`/`<dt>`/`<dd>` grid (Status / Profiles); loading branch keeps `Card` chrome with `text-ink-3` Loading line; uses Forge `Button`/`Card`/`Field`/`Toggle` primitives + `text-ink-3` / `text-ink` tokens; no shadcn, no hex, no inline styles. Public API (`device`) preserved on 2026-05-10 (slice T4-α). |
| DeviceIdentityCard       | [x]    | always read-only key/value rows — Forge `Card` + `<Field editing={false} value=…>` per row (MAC / Hostname / Manufacturer / First seen / Last seen); `.field-value` (13px ink, 8px vertical padding) replaces ad-hoc label/value pairs; `.mono` wraps the MAC; inner `Row` helper dropped — Field's column layout owns the rhythm. Public API (`device`) preserved (slice T4-α on 2026-05-10). |
| DeviceNetworkCard        | [ ]    | edit-mode card                                   |
| DeviceSettingsCard       | [ ]    | edit-mode card                                   |
| DnsFilterSettingsCard    | [ ]    | edit-mode card with toggles                      |
| DnsStatsSection          | [ ]    | qchart + donut + stat tiles                      |
| EditDhcpConfigSheet      | [-]    | subsumed by DhcpConfigCard edit-mode (sheet dropped) |
| ManualTunnelTab          | [ ]    | tabbed sub-form                                  |
| PowerCard                | [ ]    | danger-toned actions outside card per skill §detail |
| ProviderTunnelTab        | [ ]    | tabbed sub-form                                  |
| RestartProgressDialog    | [ ]    | dialog                                           |
| ShutdownProgressDialog   | [ ]    | dialog                                           |
| TunnelDevicesTable       | [ ]    | `.tbl` + `.host`                                 |
| TunnelThroughputChart    | [ ]    | line chart §10 (Download = `--accent`, Upload = `--info`) |
| UpdateCard               | [ ]    | card                                             |

---

## Layouts — `source/admin-app/web/src/components/layouts/`

| File         | Status | Notes                                                              |
| ------------ | ------ | ------------------------------------------------------------------ |
| AppLayout    | [ ]    | floating sidebar default; topbar with breadcrumbs + search + ⌘K kbd |
| AuthLayout   | [ ]    | centered card on `--bg` (no chrome)                                |

---

## Pages — `source/admin-app/web/src/pages/`

| Page                  | Status | Forge ref                                                   |
| --------------------- | ------ | ----------------------------------------------------------- |
| Dashboard             | [ ]    | screens.jsx §01 — 9 StatTiles + DNS qchart + donut + log stream |
| Devices               | [ ]    | screens.jsx §02 — `.tbl` with `.host`                       |
| DeviceDetail          | [ ]    | detail-screens.jsx — Identity / Settings / DNS / Network cards, edit-mode protocol |
| Tunnels               | [ ]    | screens.jsx §03 — `.tcard` grid + add modal                 |
| TunnelDetail          | [ ]    | detail-screens.jsx — throughput chart + connected devices   |
| Dhcp                  | [ ]    | screens.jsx §04 — leases + reservations                     |
| Dns                   | [ ]    | screens.jsx §05 — query stats + real-time stream            |
| DnsLogs               | [ ]    | log viewer + filters                                        |
| DnsFilter             | [ ]    | screens.jsx §06 — `.cat` rows with toggles                  |
| DnsFilterProfile      | [ ]    | profile detail with rule tables                             |
| DnsFilterProfileNew   | [ ]    | profile create form                                         |
| MyDevice              | [ ]    | per-device dashboard                                        |
| Settings              | [ ]    | screens.jsx §07 — backup / power / update cards             |
| Setup (+ setup/*)     | [ ]    | wizard — first-card status pattern, multi-step              |
| Login                 | [ ]    | AuthLayout, single card                                     |
| NotFound              | [ ]    | `.empty` full-page                                          |

---

## Public site — `source/marketing-site/`

| File / area                       | Status | Notes                                                                             |
| --------------------------------- | ------ | --------------------------------------------------------------------------------- |
| `src/index.css`                   | [ ]    | replace `--brand-indigo|slate|green` with Forge tokens; swap Geist → Inter Tight + JetBrains Mono |
| `pages/Home.tsx`                  | [ ]    | Hero on `--bg-elev` w/ Ward Navy accent block; principle: numbers earn the room    |
| `pages/Docs.tsx`                  | [ ]    | sidebar + body, mono code, Forge `.kbd` style                                      |
| `pages/DocsArticle.tsx`           | [ ]    | typography pass — Inter Tight body, JBM for `<code>`                               |
| `pages/ErrorView.tsx` / `NotFound.tsx` | [ ] | `.empty` pattern                                                                   |
| `components/layouts/Navbar.tsx`   | [ ]    | top nav — Forge type scale, no chrome below                                        |
| `components/layouts/Hero.tsx`     | [ ]    | display type, accent CTA                                                           |
| `components/layouts/HowItWorks.tsx` / `Features.tsx` / `TechStack.tsx` / `GetStarted.tsx` / `Footer.tsx` | [ ] | re-skin with Forge classes & tokens |
| `components/compound/CodeBlock.tsx` | [ ]  | `.logs` family / `--bg-sunken` + `--font-mono`                                     |
| `components/compound/ErrorBoundary.tsx` | [ ] | full-page error using `.empty` + danger pill                                     |
| `components/compound/FeatureCard.tsx` | [ ] | `.card` + headline stat + sub line                                                |
| `components/compound/LatestReleaseBadge.tsx` | [ ] | `.pill` + mono version                                                       |
| `components/compound/Logo.tsx`    | [ ]    | shield + signal mark — share with admin Logo if shape allows                       |
| `components/compound/StepCard.tsx` | [ ]   | `.card` with step number in display type                                           |
| `components/compound/TechBadge.tsx` | [ ]  | `.pill--ghost` (mono)                                                              |

---

## Findings (live — append as we go)

- **Tailwind**: already `4.2.4` — no version bump. `@theme inline` block in
  `index.css` is the right home for the Forge → utility mapping.
- **Radix**: `radix-ui@1.4.3` is the unified package (`radix-ui/Dialog`,
  `radix-ui/DropdownMenu`, etc.) — no per-primitive installs. `shadcn` package
  is just the CLI/generator and can be removed once primitives are rewritten.
- **Density**: V1 bakes comfortable. The `[data-density="compact"]` block in
  Forge `styles.css` gets stripped when we vendor it — fewer dead tokens, one
  less surface to keep correct.
- **Theme strategy**: `next-themes` defaults to class (`.dark`); needs
  `attribute="data-theme"` config so Forge's token block fires.
- **Public site palette**: `source/marketing-site/src/index.css` carries its own
  `--brand-indigo|slate|green` tokens with a leading comment "Brand palette
  (matches admin web-ui)" — that comment becomes a fact once both surfaces share
  Forge tokens.
- **Fonts**: drop `@fontsource-variable/geist`; add Inter Tight + JetBrains Mono
  (variable fonts; load via `@fontsource-variable/inter-tight` +
  `@fontsource-variable/jetbrains-mono` to keep package patterns consistent).
- **Forge font-feature-settings**: body sets `"ss01", "cv11"` for Inter Tight
  alts and `"zero"` for JBM — preserve in `body` rule.
- **Sidebar**: floating is the default and the only variant we ship. The
  `.app[data-rail="floating"]` block becomes plain `.app` in our vendored CSS
  (or we keep the data attribute and always set it — trivial either way).
- **Lucide stroke**: lucide defaults to `strokeWidth={2}`. Forge uses 1.7. Set
  the project default once (`<Icon strokeWidth={1.7}>` wrapper or a lucide
  global default) so we don't sprinkle it on every call site.
- **No light/dark toggle in web-ui yet** (2026-05-09): `next-themes` is in
  `package.json` but no `<ThemeProvider>` is mounted. The only theming logic is
  `useTheme.ts`, an OS-preference sync that flipped the `.dark` class. Updated
  it to set `data-theme="dark"|"light"` on `<html>` so Forge tokens fire.
  When we add an in-app toggle, mount `next-themes` with
  `attribute="data-theme"` and the existing OS sync becomes the default.
- **Tailwind 4 errors on unknown utilities at build time** — the foundation slice
  can't simply *delete* the shadcn tokens without breaking `vite build` for every
  component still using `bg-background` / `text-foreground` / etc. Both apps now
  carry a "legacy aliases" block in `index.css` mapping those names to Forge
  equivalents. Visuals will look off in unmigrated components (e.g., shadcn's
  "accent" was a soft-hover color; Forge's `--accent` is brand green — anywhere
  a hover target uses `bg-accent` it'll render brand-green). The blocks get
  deleted incrementally as components migrate; checklist rows track each.
- **Theme attribute over class — locked** (discussed 2026-05-09): considered
  flipping Forge to `.dark` to match Tailwind defaults. Decided against:
  Forge already uses `[data-attribute]` for density and rail; switching theme
  alone to a class would mix mechanisms within Forge's own state model. Modern
  design systems (Radix, Spectrum, Primer, Pico) use `[data-theme]` for the same
  reason — N-valued state attributes scale cleaner than binary classes. The
  `@custom-variant dark (&:where([data-theme="dark"], [data-theme="dark"] *));`
  line in each app's `index.css` keeps legacy `dark:` utilities firing through
  the migration.
- **Vite alias `@wardnet/forge`** mirrors the existing `@wardnet/js` SDK portal.
  Today it resolves to `design-system/`; once `source/forge/` exists (first
  primitive slice), only the alias target moves — consumer imports
  (`@wardnet/forge/styles.css`) stay stable.
- **Site doesn't use Radix** (verified 2026-05-09): no `radix-ui`, `cmdk`, or
  `sonner` deps. Site will continue to consume only `@wardnet/forge/styles.css`
  + `@wardnet/forge/tokens` going forward — primitives belong to web-ui only.
  Subpath exports on `@wardnet/forge` are mandatory so the site bundle never
  drags Radix in.
- **Site had a JS-side font import in `main.tsx`** (`import "@fontsource-variable/geist"`)
  in addition to the CSS-side import — and a matching `declare module` in
  `vite-env.d.ts`. CSS-side `@import` is sufficient on its own; removed the
  JS side and the type declaration to keep one font-loading mechanism.
- **Forge bootstrap — Vite alias dropped, not retargeted** (2026-05-09):
  the original plan was to retarget the `@wardnet/forge` Vite alias at
  `source/forge/src`. Doing that as a prefix-replace alias short-circuits
  the package's exports map (Vite's `resolve.alias` is a substring rewrite,
  so `@wardnet/forge/button` would map to `source/forge/src/button` and miss
  `source/forge/src/primitives/button.tsx`). With subpath exports mandatory,
  the alias and the exports map are mutually exclusive. The cleaner path is
  to drop the alias in both apps and let yarn's `portal:` protocol + the
  package's `exports` map do the resolution — which is what `@wardnet/js`
  already does without an alias. CSS-side `@import "@wardnet/forge/styles.css"`
  goes through the same resolver and works unchanged.
- **`preserveSymlinks: true` required for portal'd source-package consumers**
  (2026-05-09): when web-ui imports `@wardnet/forge/button`, yarn symlinks
  the package into `web-ui/node_modules/@wardnet/forge`, but tsc and Vite
  default to resolving symlinks to their real path
  (`source/forge/src/primitives/button.tsx`). From there, walking up looking
  for `react` lands in `source/forge/node_modules`, which does not have it.
  The fix is `preserveSymlinks: true` — set in `web-ui/tsconfig.app.json`
  for type-checking and in `web-ui/vite.config.ts` (`resolve.preserveSymlinks`)
  for bundling. With the flag on, walking up from the symlinked path lands
  in `web-ui/node_modules`, where react and its types live. Without it,
  forge would need its own copy of every transitive dep. Site doesn't import
  any primitive `.tsx` — it only consumes the CSS — so it doesn't need the
  flag (but if/when site imports a primitive, we'll need it there too).
- **Forge package layout — peer-deps for React, no `@types/*` devDeps**
  (2026-05-09): `react` and `react-dom` declared as peers (consumer
  provides), no devDeps for them. `@types/react` was tried as a forge
  devDep first; that produced "two different `@types/react` exist with
  this name" type errors, because tsc was loading both forge's copy and
  web-ui's copy. Removing them from forge and relying on `preserveSymlinks`
  to reach the consumer's types fixed it. Side-effect: forge's own
  `tsc --noEmit` won't run standalone (no react types reachable from
  forge alone) — type-checking happens through the consumer apps, which
  is fine for the workspace-package shape.
- **`radix-ui` moved from web-ui deps to forge deps** (2026-05-09): apps
  consume primitives, primitives consume Radix. With nodeLinker `node-modules`
  and `preserveSymlinks: true`, web-ui's not-yet-ported shadcn wrappers
  (alert-dialog.tsx, dialog.tsx, etc.) can still `import { ... } from "radix-ui"`
  — yarn hoists the transitive dep into web-ui/node_modules. Once those
  wrappers are ported into forge, web-ui's source no longer touches Radix
  directly.
- **Card primitive — export shape locked: named exports** (2026-05-09).
  Considered the compound-component pattern (`Card.Header`, `Card.Body`)
  which is more idiomatic and matches how Radix exposes parts, but settled
  on flat named exports (`Card`, `CardHeader`, `CardTitle`, `CardDescription`,
  `CardAction`, `CardContent`, `CardFooter`) for two reasons: (1) it
  matches the 29 existing call sites' shape so the migration is a
  pure import-path retarget — no JSX rewrite — keeping the slice tight;
  (2) Radix-wrapping primitives we'll port later (Dialog, Tabs, DropdownMenu)
  can still expose their compound-y feel via separate exports
  (`DialogRoot`, `DialogTrigger`, `DialogContent`) without forcing a
  different convention on the non-Radix primitives. **Rule for future
  primitives: named exports unless there's a concrete reason otherwise.**
- **Composition implies layout — primitives don't expose escape-hatch
  props** (2026-05-09, locked while porting Card). The first draft of the
  Card primitive had a `flush` boolean prop wrapping Forge's
  `.card--flush` modifier. Caught before commit: a prop like that lets a
  consumer set padding behaviour independently of the card's actual
  structure, which is exactly the kind of door that lets app code
  diverge from the design system. The right shape is to derive layout
  from composition: a card with a `<CardHeader>` or `<CardFooter>` is
  flush by construction (those parts own their padding; the parent's
  default padding would double up). Implemented in CSS so the rule lives
  with the styling:
  ```css
  .card:has(> .card__head),
  .card:has(> .card__foot) { padding: 0; }
  ```
  `.card--flush` stays in Forge as an explicit class for cases without
  head or foot (image-only card, table-only card) and CSS-only consumers,
  but the React primitive doesn't surface it as a prop. **Rule for future
  primitives: don't give consumers props that let them choose visual
  behaviour Forge already decides from structure.** Tradeoff acknowledged:
  `:has()` requires Safari 15.4+ / Chrome 105+ / Firefox 121+ — fine for
  the admin app (modern evergreen browsers) but worth flagging if we ever
  target older runtimes.
- **Variant naming — Forge vocabulary when semantic, legacy when stylistic**
  (2026-05-09, locked while porting Pill). Button kept its legacy variant
  strings (`outline`/`secondary`/`ghost`/`destructive`/`tertiary`) because
  those names are stylistic — they describe how the button looks, not what
  it means — and Forge has 1:1 visual mappings (`.btn--ghost` etc.) that
  translate cleanly via CVA without renaming call sites. Pill is different:
  the legacy variants (`success`/`destructive`/`outline`) are
  domain-flavoured, and Forge introduces *new* semantic variants
  (`warn`, `info`) that the app should be using. Adopting Forge's
  `ok|warn|down|info|ghost` here both renames AND enriches — call sites can
  now express `warn` and `info` properly. **Rule for future primitives:**
  when Forge has a richer or more semantic variant vocabulary than the
  legacy primitive, adopt Forge's names and migrate call sites; when
  Forge variants are 1:1 visual stand-ins for legacy stylistic variants,
  keep legacy names and CVA-map them. The size of the call-site graph
  matters too — Button had 44 sites, Pill had 9 — so the
  reasonable-friction threshold for renaming flexes by primitive.
- **Component name follows the Forge class name** (2026-05-09, Pill slice).
  The legacy file was `badge.tsx` exporting `<Badge>`, but Forge calls it
  `.pill`. Renamed to `Pill` so the React vocabulary matches the CSS
  vocabulary — keeps the design system coherent across layers. Future
  ports follow the same rule (Toggle, not Switch; Modal, not Dialog —
  modulo Radix conventions where the Radix name is the lingua franca).
- **`.card__foot` added to Forge** (2026-05-09). The legacy shadcn `Card`
  exposed a `CardFooter` styled with Tailwind utilities; Forge had no
  matching class. Per the "Forge first" rule, the slice that needed it
  added it to `source/forge/styles.css` (next to `.card__head`) rather
  than emitting bespoke styles inside the primitive. Symmetric with the
  head: padding, border, sunken background, plus a nested `.right`
  selector for right-aligned actions. `.card--flush` is no longer needed
  for a footer-bearing card — the `:has()` rule above takes care of it.
- **Component name always follows Forge class name — no Radix exception**
  (2026-05-09, Toggle slice — supersedes the Pill slice's "modulo Radix
  conventions" wording). Earlier the Pill finding carved out an
  exception "where the Radix name is the lingua franca." On porting
  Switch we walked that back: Radix's sub-library names
  (`Switch.Root`, `Dialog.Trigger`, `Tabs.Content`) belong inside the
  primitive's implementation file — that's where you're staring at
  Radix docs — but they don't earn the export name. The rule is now
  flat: **the file, the export, and the subpath are Forge's name**.
  Toggle, not Switch (`toggle.tsx`, `./toggle`, `<Toggle …/>`); Modal,
  not Dialog (when we get there); Pill, not Badge. The Radix umbrella
  import keeps its Radix name inside the file (`import { Switch } from
  "radix-ui"`, then render `<Switch.Root … />`). Future Radix-wrap
  slices follow the same pattern: Dialog (Radix) → Modal (Forge
  `.modal`); DropdownMenu likely keeps its name (no Forge collision);
  Tabs stays Tabs (Forge `.tabs`).
- **Radix `data-state` ↔ Forge modifier classes — bridged in CSS**
  (2026-05-09, Toggle slice — sets the template for every state-bearing
  Radix-wrap that follows: Tabs, RadioGroup, DropdownMenu, Popover,
  Dialog→Modal, Sheet, AlertDialog, Select). Forge expresses primitive
  state through modifier classes (`.toggle.is-on`, etc.) authored
  alongside the CSS-only mocks. Radix expresses the same state through
  `data-state="checked|unchecked|open|closed|active"` attributes
  rendered onto its primitive root. We bridge the two **in Forge's
  stylesheet**, not in JS-side class juggling, by giving the modifier
  rule a dual selector:
  ```css
  .toggle.is-on,
  .toggle[data-state="checked"] { background: var(--accent); }
  .toggle.is-on::after,
  .toggle[data-state="checked"]::after { left: 18px; }
  ```
  CSS-only consumers (the `forge/docs/` studio mocks) keep using
  `.is-on`; the React primitive renders the bare Forge class on Radix
  Switch.Root and lets Radix's `data-state="checked"` flip the visual.
  No JS-side state→className computation, no doubled selector logic
  inside the primitive. **Rule:** Forge will grow a family of
  `[data-state="…"]` selectors alongside its existing modifier classes
  as Radix-wrap primitives land — that is deliberate and correct, not
  drift. Add the dual selector in the same slice that introduces the
  Radix-wrapped primitive.
- **Radix umbrella import shape — `radix-ui` + `Sub.Root`**
  (2026-05-09, Toggle slice). Verified by reading the legacy
  `core/ui/switch.tsx`: Radix's unified package is imported as
  `import { Switch } from "radix-ui"` (not `import * as Switch from
  "radix-ui/react-switch"`), and the parts are accessed as
  `Switch.Root`, `Switch.Thumb`, etc. This matches the existing Button
  primitive's `import { Slot } from "radix-ui"` shape. **Template for
  future Radix-wrap primitives:** import the sub-library by its Radix
  name from `"radix-ui"`, render its parts as `Sub.Root` /
  `Sub.Trigger` etc. inside the file; export under the Forge name.
- **Forge thumbs come from Forge** (2026-05-09, Toggle slice). Radix
  Switch traditionally renders a `<Switch.Thumb>` child for the
  draggable knob; Forge's `.toggle::after` already provides it. The
  Toggle primitive renders `<Switch.Root>` with no children — adding
  `Switch.Thumb` would double up the visual. Generalises to other
  Radix primitives where Forge's CSS owns a sub-element via
  pseudo-element: don't render the Radix sub-component if Forge
  already provides the visual.
- **Legacy shadcn alias audit — Toggle slice** (2026-05-09). The
  deleted `switch.tsx` referenced `bg-input`, `bg-primary`,
  `bg-background`, `bg-foreground`, `bg-primary-foreground`,
  `border-ring`, `ring-ring`, `border-destructive`, `ring-destructive`.
  None of those alias rows in `admin-app/web/src/index.css` reached
  zero references after deletion — they're all still used by other
  unmigrated components. Pre-existing zero-referenced rows
  (`card-foreground`, `secondary`, `secondary-foreground`,
  `destructive-foreground`, `sidebar-primary`,
  `sidebar-primary-foreground`, `sidebar-ring`) were already zero
  before this slice; the alias-pruning slice can drop them whenever
  it lands.
- **Multi-part Radix-wrap template — Modal slice** (2026-05-09, fifth
  primitive). Modal is the first primitive that wraps a *multi-part*
  Radix sub-library (`Dialog.Root` / `Dialog.Trigger` / `Dialog.Portal` /
  `Dialog.Overlay` / `Dialog.Content` / `Dialog.Title` /
  `Dialog.Description` / `Dialog.Close`) and exposes a multi-part
  React surface (`Modal` / `ModalTrigger` / `ModalContent` /
  `ModalHeader` / `ModalTitle` / `ModalDescription` / `ModalBody` /
  `ModalFooter` / `ModalClose`). The mapping rule that emerged: each
  exported part is one of two shapes — **(a) "passthrough Radix"**:
  `Modal` (Dialog.Root), `ModalTrigger` (Dialog.Trigger),
  `ModalDescription` (Dialog.Description), `ModalClose` (Dialog.Close)
  — these have no Forge visual surface of their own and pass through
  to Radix verbatim, so the primitive is a one-line forwarder.
  **(b) "Forge-owned div"**: `ModalHeader` = `<div className="modal__head">`,
  `ModalBody` = `<div className="modal__body">`, `ModalFooter` =
  `<div className="modal__foot">` — these don't correspond to any
  Radix sub-component because Forge owns those layouts visually
  (extends the Toggle slice's "don't render Radix sub-components
  Forge already owns visually" rule from a single pseudo-element to
  whole structural divs). **(c) "Compose Radix into Forge structure"**:
  `ModalContent` renders `<Dialog.Portal>` + `<Dialog.Overlay
  className="scrim">` + `<Dialog.Content className="modal">` —
  three Radix parts collapsed into one consumer-facing export
  because Forge treats backdrop+surface as a single "modal content"
  unit. **(d) "Radix part wearing Forge's expected element"**:
  `ModalTitle` renders `<Dialog.Title asChild><h3>...</h3></Dialog.Title>`
  so Radix's a11y wiring (aria-labelledby) lands on the h3 that
  Forge's `.modal__head h3` selector already styles. Same trick will
  work whenever Radix's default element doesn't match Forge's
  expected element. Future multi-part wraps (AlertDialog, Sheet,
  Popover, DropdownMenu, Select) follow this four-shape map — list
  each export, classify it, and the implementation falls out.
- **`data-state="open"|"closed"` is a generalisation of the Toggle
  slice's `checked|unchecked` rule** (2026-05-09, Modal slice). Toggle
  bridged Radix's `data-state="checked"` to Forge's `.is-on` modifier
  with a dual selector. Modal repeats the pattern for `data-state="open"`
  (entrance) and `data-state="closed"` (exit) on `.scrim` and `.modal`.
  Confirms the rule generalises across all state-bearing Radix
  primitives — every Radix-wrap slice that introduces state grows
  Forge's `[data-state="…"]` selector family in the same commit.
  Concrete additions in this slice:
  - `.scrim` and `.modal` each got `[data-state="open"]` / `[data-state="closed"]`
    selectors driving entrance/exit animations.
  - Forge's existing `pop` keyframe was renamed to `scrim-in` and a
    matching `scrim-out` keyframe added; new `modal-in` / `modal-out`
    keyframes handle the centered surface (fade + small upward
    translate + 0.97→1 scale).
  - `.modal` was given `position: fixed; top: 50%; left: 50%;
    transform: translate(-50%, -50%)` so it can render as a *sibling*
    of `<Dialog.Overlay>` (Radix's default tree shape) rather than
    as a child of `.scrim`. Side selector `.scrim > .modal { position:
    static; transform: none }` keeps the scrim's grid centering working
    for CSS-only mocks where the modal is nested inside the scrim.
- **Animations live in Forge when they're intrinsic to `.scrim` /
  `.modal`** (2026-05-09, Modal slice). The stack-changes table keeps
  `tw-animate-css` for "dialog/sheet/dropdown entrance animations." We
  considered whether the entrance/exit fade/scale belongs in Forge's
  stylesheet or in the primitive's `className` props (using
  tw-animate-css's `data-[state=open]:animate-in` /
  `data-[state=closed]:animate-out` Tailwind utilities). Decision: when
  the animation describes a property of the class itself (every modal
  fades in; every scrim fades the backdrop in), it belongs in Forge —
  CSS-only mocks and the React primitive then share visuals without
  tw-animate-css being a runtime concern for either. tw-animate-css
  remains in the dependency graph for variant-shaped animations that
  WILL belong in the primitive — e.g. Sheet's slide-from-bottom is a
  direction-specific extension of Modal's centered fade and reads
  more cleanly as `data-[state=open]:slide-in-from-bottom` next to
  Sheet-specific Tailwind classes than as a separate Forge keyframe.
  **Rule:** intrinsic visuals → Forge keyframes scoped to
  `[data-state="…"]`; variant-specific or composition-specific motion
  → tw-animate-css utilities on the primitive's className.
- **`Dialog.Trigger` already supports `asChild` natively — no Slot
  wrapper needed in Modal** (2026-05-09, Modal slice). Earlier Button
  added an explicit Radix `Slot.Root` wrapper for `asChild`. For
  Modal's `ModalTrigger`, the equivalent prop is provided by Radix
  itself — `<Dialog.Trigger asChild>` is part of the Radix Dialog API.
  ModalTrigger is a one-line passthrough that re-exposes
  `React.ComponentProps<typeof Dialog.Trigger>`, including `asChild`,
  to consumers. Same applies to `Dialog.Close` (and `Dialog.Title`
  via the asChild trick used in ModalTitle). **Rule:** when the Radix
  sub-component already supports `asChild`, the primitive doesn't
  need a separate `Slot` import — Radix's API is the contract.
- **Legacy shadcn alias audit — Modal slice** (2026-05-09). The
  deleted `dialog.tsx` referenced `bg-background`, `bg-muted/50`,
  `text-muted-foreground`, plus shadcn animation utilities
  (`data-open:animate-in` / `data-closed:animate-out` /
  `data-open:fade-in-0` / `data-open:zoom-in-95` etc., which are
  tw-animate-css aliases, not shadcn token aliases). None of the
  shadcn token alias rows in `admin-app/web/src/index.css` reached
  zero references after deletion — `bg-background` (6),
  `text-foreground` (32), `bg-muted` (31), `text-muted-foreground`
  (231), `bg-popover` (7), `text-popover-foreground` (5) all retain
  consumers. The alias-pruning slice still has work to do.
- **AlertModal — Forge needs no new modifier class for "alert"
  framing** (2026-05-09, sixth primitive). Going in, the table row
  and the migration plan both expected a `.modal--danger` /
  `.modal--alert` modifier in Forge to encode AlertDialog's visual
  delta. Reading `core/ui/alert-dialog.tsx` first dissolved the
  premise: legacy `AlertDialogAction` defaulted to `variant="default"`
  (NOT `destructive`) and accepted a `variant` prop, so danger
  styling was always per-call-site on the *button*, never on the
  modal surface. Visually `<AlertDialogContent>` and
  `<DialogContent>` were token-for-token identical — both centered,
  same `bg-popover`, same `ring-1 ring-foreground/10`, same
  `data-open:zoom-in-95` motion. So Forge ships `.modal` and
  nothing more; alert framing is encoded in (a) Radix's behavioral
  semantics (role="alertdialog", forced confirmation, overlay-click
  suppressed), and (b) per-call-site `<Button variant="destructive">`
  on the action. The export name is `AlertModal` because that's the
  meaningful *behavioral* variant — the `.modal` class anchors the
  visual surface (rule honored), and the `Alert` prefix denotes the
  Radix forced-confirmation behavior that Forge doesn't speak to.
  **Generalisation:** when a Radix sub-library adds *behavior* on
  top of an already-styled Forge surface, name the React export
  after the behavior (Alert prefix) without inventing a new Forge
  class. New Forge classes are reserved for *visual* variants that
  CSS-only mocks would also want.
- **AlertModal Action/Cancel are pure Radix passthroughs — Button
  is composed by the consumer via `asChild`** (2026-05-09, sixth
  primitive). Legacy `AlertDialogAction` and `AlertDialogCancel`
  baked a `<Button variant={…}>` wrapper into the primitive,
  surfacing a `variant` prop pulled from Button's API. Walked it
  back: that's exactly the escape-hatch-prop pattern the Card
  slice locked against — it lets a consumer set button styling
  inside the alert primitive, fragmenting Forge's button vocabulary
  across call sites. Cleaner shape: the primitive forwards
  `<AlertDialog.Action>` / `<AlertDialog.Cancel>` verbatim and
  consumers compose with `<AlertModalAction asChild><Button …>…</Button></AlertModalAction>`.
  Cost: every call site grows by one wrapper element per action/cancel
  (4 dialogs × ~2 buttons each ≈ 8 sites). Benefit: the primitive
  has zero opinion about which `Button` variant is appropriate —
  destructive on shutdown, default on reboot, outline on dismiss —
  and that opinion lives entirely at the call site where context
  exists. Same pattern will apply to Sheet / Popover / DropdownMenu
  triggers and any other Radix part that traditionally pairs with
  a styled trigger button. **Rule:** primitives that wrap Radix
  parts whose default rendering is "button-like" do NOT bake a
  `<Button>` wrapper — they pass through and let consumers
  `asChild` a Forge Button.
- **AlertDialog vs Dialog — what actually differs** (2026-05-09,
  sixth primitive). Verified by reading Radix docs + the legacy
  wrapper: AlertDialog differs from Dialog in three ways, none
  visual: (a) `role="alertdialog"` (vs `dialog`) — screen readers
  announce it as a forced-decision modal; (b) overlay click is
  ignored — the only ways out are Action, Cancel, or programmatic
  close; (c) `<AlertDialog.Action>` and `<AlertDialog.Cancel>`
  exist (vs Dialog's single `<Dialog.Close>`) — they're separate
  parts so Radix can wire focus management to the Cancel
  (recommended initial focus). Forge's `.modal` class doesn't
  encode any of this — all three differences are runtime behavior
  Radix owns. The only place these differences surface in the
  primitive is the import (`AlertDialog` instead of `Dialog`),
  the parts (`Action` + `Cancel` instead of `Close`), and the
  absence of an `AlertModalClose` export (Radix.AlertDialog has
  no `Close` part).
- **Multi-part template generalises cleanly to AlertDialog**
  (2026-05-09, sixth primitive). Modal locked the four-shape
  classification (passthrough Radix / Forge-owned div / composed
  Radix tree / Radix-part-wearing-Forge-element-via-asChild).
  AlertModal slots into the same four shapes with no new shape
  needed: `AlertModal` / `AlertModalTrigger` / `AlertModalDescription` /
  `AlertModalAction` / `AlertModalCancel` are passthrough Radix
  (5 parts); `AlertModalHeader` / `AlertModalBody` / `AlertModalFooter`
  are Forge-owned divs (`.modal__head` / `.modal__body` /
  `.modal__foot` — same classes as Modal); `AlertModalContent`
  composes `<AlertDialog.Portal>` + `<AlertDialog.Overlay
  className="scrim">` + `<AlertDialog.Content className="modal">`;
  `AlertModalTitle` is `<AlertDialog.Title asChild><h3>` — same
  trick as ModalTitle, same reason (Forge's `.modal__head h3`
  selector already styles it). State-bridge selectors (`.scrim` /
  `.modal` `[data-state="open"|"closed"]`) inherited unchanged from
  the Modal slice — verified by reading the existing styles.css
  rules; no new selectors needed. **Confirms the template
  generalises to multi-part Radix wraps that share Forge surfaces
  with another primitive.** Sheet (next slice) is the variant test
  — same .scrim/.modal but slide-from-bottom motion, which is where
  tw-animate-css will land per the Modal-slice animation rule.
- **Legacy shadcn alias audit — AlertModal slice** (2026-05-09).
  The deleted `alert-dialog.tsx` referenced `bg-popover`,
  `text-popover-foreground`, `bg-muted` (in `bg-muted/50`),
  `text-muted-foreground`, `ring-foreground/10`, `bg-black/10`,
  plus tw-animate-css utilities. None of the shadcn alias rows
  reached zero after deletion — `bg-popover` (7→6),
  `text-popover-foreground` (5→4), `bg-muted` (31→29),
  `text-muted-foreground` (231→230). Counts dropped slightly but
  the alias-pruning slice still has consumers to migrate.
- **Sheet dropped from design — replaced by inline detail/edit
  pattern** (2026-05-09, after the AlertModal slice). Originally the
  primitive table listed `sheet.tsx` as a Dialog-with-slide-from-bottom
  port targeting Forge's `mobile.html §sheet`. Walked it back: sheets
  fragment the navigation model — they slide a second surface on top
  of the page so the user has to mentally hold two contexts (the page
  underneath + the sheet) just to edit one record. Inline detail/edit
  is the same affordance with one fewer layer: the page itself
  expands the row / card / form into edit mode, then collapses back.
  Same affordance, less chrome, and it falls out of the
  edit-mode-card protocol that DhcpConfigCard / DeviceSettingsCard /
  BackupCard already use. Ripples:
  - `sheet.tsx` primitive marked `[-]` (Removed from scope; new
    legend row added).
  - `EditDhcpConfigSheet` marked `[-]` — its functionality folds
    into `DhcpConfigCard` edit-mode.
  - `CreateReservationSheet` notes updated — likely renamed to
    `CreateReservationInline` (precedent: `CreateTunnelInline`).
    Component itself ports in its own slice.
  - `MobileMenu` notes updated — without Sheet, mobile-nav needs a
    new approach (own slice will decide).
  - `tw-animate-css` justification narrows: Modal/AlertModal motion
    is in Forge keyframes; Sheet's slide-from-bottom (the canonical
    variant-specific-motion example in the Modal slice's animation
    rule) no longer exists as a use case. Whether DropdownMenu /
    Popover / Select land Forge keyframes or tw-animate-css will be
    settled by the Popover slice; until then `tw-animate-css` stays
    in deps.
  - The multi-part-Radix-wrap template (passthrough Radix /
    Forge-owned div / composed Radix tree / Radix-part-wearing-
    Forge-element-via-asChild) still holds for Popover, DropdownMenu,
    Select — Sheet would have been the variant test for slide motion;
    instead Popover (the next slice) becomes the floating-surface
    test, and the rule covering intrinsic-vs-variant motion gets
    re-examined there.
- **Popover — Forge gains `.popover` (no `.pop`)** (2026-05-09,
  seventh primitive). Forge had no class for floating non-modal
  surfaces; the `--shadow-pop` token existed (used by `.toast`)
  and the legacy `pop` keyframe shipped a fade+rise that `.toast`
  consumes, but no `.pop` / `.popover` / `.menu` class. Considered
  naming the new class `.pop` to mirror the existing `pop`
  keyframe + `--shadow-pop` token (terse, consistent with toast's
  vocabulary), but the React-export side reads weirdly as
  `<Pop>`, and the locked rule "component name follows Forge class
  name — no Radix exception" (Toggle slice) cuts both ways: if the
  React export should be `Popover` (matches Radix lingua franca
  inside the file, and `<Popover>` is what the rest of the
  ecosystem calls it), then the Forge class is `.popover` so the
  rule holds. The `--shadow-pop` token name and `pop` keyframe
  stay as they are — they're token / animation primitives, not
  component classes; the rule binds component classes to React
  components, not every CSS identifier. **Generalisation:** when
  picking a name for a new Forge class, prefer the unambiguous
  React-vocabulary name (`.popover`, `.modal`, `.toggle`) over
  terse forms; tokens and keyframes can keep their own
  shorter / pre-existing names.
- **Popover entrance motion is intrinsic — Forge keyframes scoped
  to `[data-state="…"]` + `[data-side="…"]`** (2026-05-09, seventh
  primitive — applies the Modal slice's animation rule). Modal
  locked: intrinsic motion → Forge keyframes; variant-specific
  motion → tw-animate-css. Popover's entrance is side-aware
  (slide-in from the anchor side: trigger-on-top → popover slides
  in from below; trigger-on-bottom → from above, etc.) — this is
  intrinsic to popover-style floating surfaces, not a per-call-site
  variant. Fits the Modal rule cleanly: stays in Forge. Implemented
  by extending the existing `[data-state="open"|"closed"]` family
  with a parallel `[data-side="top|right|bottom|left"]` family that
  swaps the open keyframe (`popover-in-top` / `popover-in-bottom` /
  `popover-in-left` / `popover-in-right`) while close stays
  direction-agnostic (`popover-out` — fade + zoom, no slide). The
  legacy primitive used Radix's `--radix-popover-content-transform-
  origin` CSS var to anchor scale to the trigger edge; Forge
  honors it via `transform-origin: var(--radix-popover-content-
  transform-origin, center)`, with a `center` fallback for
  CSS-only consumers (no Radix var present). **Generalisation
  for DropdownMenu / Select:** Radix exposes the same
  `data-side` attribute on `DropdownMenu.Content` /
  `Select.Content`; both should reuse the `.popover` class +
  these data-side keyframes rather than ship parallel selectors.
  Confirms Sheet's removal didn't sink the intrinsic-vs-variant
  rule — Popover is the cleaner test case anyway because the
  motion is geometric (anchor-side), not modal (slide-from-bottom).
  `tw-animate-css` not used here; whether DropdownMenu / Select
  / future floating surfaces ever land utility-side motion is now
  open until a concrete need surfaces.
- **Popover surface stripped to three exports** (2026-05-09,
  seventh primitive). Legacy `core/ui/popover.tsx` exported seven
  parts — `Popover`, `PopoverTrigger`, `PopoverContent`,
  `PopoverAnchor`, `PopoverHeader`, `PopoverTitle`,
  `PopoverDescription`. Both call sites (CountryCombobox,
  CronSchedulePicker) imported only `Popover` / `PopoverTrigger`
  / `PopoverContent`; the other four had zero consumers. Stripped
  per the locked rule "drop features the migration doesn't
  require." If a future call site needs `Anchor` (decoupled
  positioning) or header/title/description, those add back as
  needed — Radix's `Popover.Anchor` is one line, and the legacy
  Header/Title/Description were Forge-owned divs with trivial
  Tailwind classes (`flex flex-col gap-0.5 text-sm`,
  `font-medium`, `text-muted-foreground`) that don't survive a
  Forge port unchanged anyway.
- **Multi-part template — Popover lands cleanly in three of the
  four shapes; no Title/Description = no asChild trick this
  slice** (2026-05-09, seventh primitive). Modal locked the
  four-shape classification (passthrough Radix / Forge-owned div
  / composed Radix tree / Radix-part-wearing-Forge-element-
  via-asChild). Popover slots into three: `Popover` and
  `PopoverTrigger` are passthrough Radix; `PopoverContent`
  composes `<Popover.Portal>` + `<Popover.Content>` (no Overlay
  — Popover is non-modal, no scrim, no focus trap). The fourth
  shape (Radix-part-wearing-Forge-expected-element via asChild)
  doesn't show up because the stripped surface has no Title /
  Description / Header. Sub-component count is the variable —
  Modal had nine exports across all four shapes; Popover has
  three across two. **Confirms the template scales down as well
  as up** — DropdownMenu (next, ten-plus parts) will stress the
  template upward; Popover stresses it downward.
- **Legacy shadcn alias audit — Popover slice** (2026-05-09).
  The deleted `popover.tsx` referenced `bg-popover` (one
  consumer here), `text-popover-foreground` (one),
  `text-muted-foreground` (one), plus `ring-foreground/10` and
  Tailwind animation utilities. None of the shadcn alias rows
  reached zero after deletion — `bg-popover` (6→5),
  `text-popover-foreground` (4→3), `text-muted-foreground`
  (230→ish — same order of magnitude). Counts continue to drop
  slowly; the alias-pruning slice still has consumers to migrate.
- **DropdownMenu — legacy is much smaller than expected; no
  CheckboxItem / RadioItem / Sub\* / Group / Label /
  ItemIndicator / Shortcut anywhere** (2026-05-09, eighth
  primitive). Going-in scope listed ten-plus parts (full Radix
  surface). Reading the legacy `core/ui/dropdown-menu.tsx`
  collapsed that to five exports — `DropdownMenu`,
  `DropdownMenuTrigger`, `DropdownMenuContent`,
  `DropdownMenuItem` (with a `variant: "default" | "destructive"`
  prop), `DropdownMenuSeparator`. All three call sites
  (`AllowlistTable`, `FilterRuleTable`, `BlocklistTable`) use
  exactly those five. So the briefing's open Decisions 3
  (CheckboxItem / RadioItem indicator strategy) and 4 (Sub\*)
  dissolve trivially: those parts had zero consumers, so they
  drop per the locked rule "drop features the migration doesn't
  require" (same calculus that stripped Popover from seven to
  three exports). If a future call site needs a checkable item
  or a sub-menu, it adds back as needed; the Forge-pseudo-
  element-vs-ItemIndicator question is a future-slice problem,
  not this one. **Generalisation:** "stress-tests the multi-part
  template upward" was the wrong frame — the upward stress is
  driven by what *consumers* use, not what Radix exposes. Modal
  remains the largest port (nine exports across all four
  shapes); DropdownMenu lands at five exports across two
  shapes (passthrough + Forge-class-bearing Radix part). The
  template still holds; the **eight Radix sub-components the
  legacy never exposed are not part of "the template upward"**
  — they're features Radix offers that the codebase doesn't
  consume.
- **DropdownMenu — `.popover` surface reused; new `.menu-item` /
  `.menu-separator` classes own the row visual** (2026-05-09,
  eighth primitive). Decision 2 from the briefing landed on
  option (a): `DropdownMenuContent` renders `<RadixDropdownMenu.
  Content className="popover">` — same surface class as the
  Popover slice introduced, no parallel `.menu` class. Items
  and separators don't share a class with anything in Forge yet
  (Popover's content was unstructured), so two new classes
  landed: `.menu-item` (`6px 8px` padding, `radius-sm`,
  `data-highlighted` flips background to `--bg-sunken`,
  `data-disabled` reduces opacity, `data-variant="destructive"`
  uses `--danger` for the rest state and `--danger-soft` /
  `--danger-soft-ink` for the highlighted background) and
  `.menu-separator` (`1px` line, `--line` token, `4px -6px`
  margin to bleed past the content's `10px` padding so the
  separator visually spans full width). Naming follows the
  Toggle/Modal/Popover rule (component class follows React
  vocabulary), but here the React export `DropdownMenuItem`
  maps to `.menu-item` rather than `.dropdown-menu-item` —
  the shorter `.menu-` prefix is the natural class name for
  *items inside any menu surface*, which lets Select reuse
  `.menu-item` later if its options match the same visual
  (likely — both are `data-highlighted`-driven row patterns).
  **Generalisation:** the rule is "Forge class follows
  React-vocabulary noun," not "Forge class is a literal
  prefix-match of the React component name" — `Item` becomes
  `menu-item` because the noun is "menu item," not "dropdown
  menu item."
- **DropdownMenu — `.popover` keyframes inherited verbatim;
  Popover's intrinsic-side-aware-motion claim holds for
  menus** (2026-05-09, eighth primitive). The Popover slice
  predicted "Radix exposes the same `data-side` attribute on
  `DropdownMenu.Content` / `Select.Content`; both should
  reuse the `.popover` class + these data-side keyframes
  rather than ship parallel selectors." Confirmed by reusing
  `className="popover"` on `RadixDropdownMenu.Content`: Radix
  emits `data-state="open|closed"` and `data-side="top|right|
  bottom|left"` on the rendered element identical to its
  Popover counterpart, so the four `popover-in-{bottom,top,
  left,right}` keyframes + `popover-out` apply unchanged.
  **No new keyframes added this slice; no `tw-animate-css`
  used.** Confirms the Popover-slice prediction and shrinks
  the open question further: only Select's content is left
  before the question of whether `tw-animate-css` can be
  removed entirely is settled. (Select's content is also a
  Radix-positioned floating surface with `data-side`; if it
  too lands `.popover`, Sheet's removal + Modal/Popover/
  DropdownMenu/Select all on Forge keyframes leaves
  zero `data-[state=…]:animate-*` callers. That's the Select
  slice's call.)
- **DropdownMenu — destructive item modeled on the data-attribute
  bridge, not a class modifier** (2026-05-09, eighth primitive).
  Legacy item carried `variant="default" | "destructive"` as a
  React prop and applied conditional Tailwind classes inline.
  Two ways to encode the same thing in Forge: (a)
  `.menu-item--destructive` modifier class, toggled by the
  primitive based on the prop; (b) `data-variant="destructive"`
  attribute, with CSS targeting `[data-variant="destructive"]`.
  Picked (b) — same family as the existing `[data-state="…"]` /
  `[data-side="…"]` / `[data-highlighted]` / `[data-disabled]`
  bridges, so the menu-item's CSS is one consistent attribute
  vocabulary instead of mixing `data-*` (for Radix-driven
  state) with `--modifier` classes (for component-driven
  variants). The primitive sets `data-variant={variant}` on
  every item; CSS-only mocks can drop the attribute on a `<div
  class="menu-item" data-variant="destructive">` and get the
  same visual. **Generalisation:** prop-driven visual variants
  on Radix-wrap primitives prefer `data-*` attributes over
  modifier classes when the primitive already inherits other
  `data-*` selectors from Radix — keeps the CSS surface
  uniform.
- **Legacy shadcn alias audit — DropdownMenu slice** (2026-05-09).
  The deleted `dropdown-menu.tsx` referenced `bg-popover`,
  `text-popover-foreground`, `bg-muted`, `bg-border`,
  `text-destructive`, `ring-foreground/10`, plus tw-animate-css
  utilities. **Two alias rows are now down to two consumers
  each** — `bg-popover` (5→2 — `select.tsx` and `command.tsx`)
  and `text-popover-foreground` (3→2 — same two files). Both
  rows will reach **zero** when Select and Command port (Select
  is the next planned slice; Command is post-tabs/forms). Other
  rows still have many consumers — `text-muted-foreground` (74
  files), `bg-muted` (20 files). **Flagging for the alias-
  pruning slice:** `bg-popover` and `text-popover-foreground`
  are both two-consumer rows with both consumers in the
  primitive-port queue, so the next two slices that touch
  `select.tsx` + `command.tsx` set them up for deletion.
- **Select — Forge gains `.select-trigger` + `.select-content`;
  options reuse `.menu-item` and the side-aware-keyframe claim
  holds for the third floating surface in a row** (2026-05-09,
  ninth primitive). Decision 1 (trigger) landed on a new
  `.select-trigger` Forge class rather than a `.field`-family
  reuse — `.field input/select/textarea` is descendant-scoped to
  `.field` and assumes a labelled column layout, but Select's
  trigger is a free-standing button-like control that needs the
  same input visuals (`--bg-elev` background, `--line` border,
  8px radius, accent focus ring) plus a flex-and-chevron layout
  the field family doesn't provide. Decisions 2 + 3 (content +
  items) reused existing classes — `SelectContent` renders
  `<RadixSelect.Content className="popover select-content">` so
  the surface lands the popover keyframes verbatim and adds a
  Select-only modifier (`min-width: var(--radix-select-trigger-
  width)`, `max-height` clamp + scroll, and a `transform-origin`
  that overrides `.popover`'s Popover-var default with the
  Select-content var); items reuse `.menu-item` with two
  Select-specific extensions (`.menu-item[data-state] {
  padding-right: 26px }` to reserve checkmark gutter, and
  `.menu-item[data-state="checked"]::after { … mask-image }` for
  the indicator). **Generalisation:** `.menu-item` continues to
  pay off as a shared row class — DropdownMenu items don't emit
  `data-state` (only `CheckboxItem`/`RadioItem` do, which we
  don't expose), so `[data-state]` selectors target only Select
  options without polluting other consumers. Two new mask-image
  pseudo-elements landed (chevron in trigger, check on selected
  option) — both with the lucide stroke geometry baked into the
  data-URI SVG, both keyed on `currentColor` (chevron also has
  a default `--ink-3` background-color that swaps for
  `currentColor` would over-darken; check inherits from the
  item's text color). The trigger's chevron rotates 180° on
  `[data-state="open"]`, picking up the existing data-state
  bridge.
- **Select — Forge owns the chevron and the indicator; the
  primitive renders zero icon JSX** (2026-05-09, ninth
  primitive). Two natural sub-components could have been
  rendered: `<Select.Icon>` (the trigger chevron) and
  `<Select.ItemIndicator>` (the checkmark on the selected
  option). Per the Toggle-slice rule "don't render Radix sub-
  components Forge already owns visually" (which also covered
  `Switch.Thumb` and the Modal slice's structural divs), both
  are encoded as Forge `::after` pseudo-elements with
  mask-image data URIs. Three reasons for going CSS-only over
  JSX-with-lucide-icons: (a) CSS-only mocks render correctly
  without React (matches Forge-as-source-of-truth principle);
  (b) the chevron's open-state rotation comes "for free" from
  the existing `[data-state="open"]` bridge instead of needing
  a JSX-level transform-on-open hack; (c) it preserves the
  no-icon-as-prop pattern (callers can't accidentally swap the
  chevron for a different glyph by passing `<Select.Icon
  asChild><FooIcon /></Select.Icon>` — composition implies
  layout, the chevron is the layout). The mask-image SVGs are
  hand-tuned to lucide's stroke-2 + round caps/joins so the
  chevron and check render visually consistent with lucide
  icons used elsewhere in the app.
- **Select — surface stripped to five exports; same calculus
  as DropdownMenu** (2026-05-09, ninth primitive). Going-in
  Radix surface is 14 sub-components (`Select.Root` / `Trigger`
  / `Value` / `Portal` / `Content` / `Viewport` / `Item` /
  `ItemText` / `ItemIndicator` / `Group` / `Label` /
  `Separator` / `ScrollUpButton` / `ScrollDownButton`).
  Surveying the 10 call sites (UpdateCard, DashboardLogWidget,
  DeviceSettingsCard, ProviderTunnelTab, DeviceSelect,
  RoutingSelector, CronSchedulePicker, DnsLogs, MyDevice,
  Step3DhcpOnboarding) showed exactly five exports in use:
  `Select`, `SelectTrigger`, `SelectValue`, `SelectContent`,
  `SelectItem`. Zero call sites use `Group` / `Label` /
  `Separator` / `ScrollUpButton` / `ScrollDownButton` — dropped
  per the locked rule "drop features the migration doesn't
  require." `Viewport`, `ItemText`, `ItemIndicator`, `Portal`
  are internal to the primitive (not exported). **Multi-part
  template:** Select lands at five exports across two shapes —
  passthrough Radix (`Select`, `SelectValue`) and Radix-part-
  bearing-Forge-class (`SelectTrigger` = `.select-trigger`,
  `SelectContent` = composed Portal+Content with `.popover
  .select-content`, `SelectItem` = composed Item+ItemText with
  `.menu-item`). Modal remains the largest port at nine across
  all four shapes; Select sits between DropdownMenu (5/2) and
  Popover (3/2) on the same template axis. Default position
  switched from legacy `"item-aligned"` to `"popper"` so the
  side-aware keyframes (which depend on Radix emitting
  `data-side`, only set in popper mode) actually trigger;
  align switched from `"center"` to `"start"` for left-edge-
  aligned dropdowns. None of the 10 call sites pass `position`
  / `align` explicitly so this is a pure default change.
- **Select — animation rule's fourth application; tw-animate-css
  removal blocked by `sheet.tsx`, not by primitives** (2026-05-09,
  ninth primitive). The Modal-slice rule "intrinsic motion lives
  in Forge keyframes scoped to `[data-state="…"]`; variant-specific
  motion uses tw-animate-css" predicted that if Select.Content
  also lands `.popover`, the dependency could be removed in this
  slice. The first half held — `.popover` keyframes inherited
  verbatim, no new keyframes added, no tw-animate-css utilities
  used in `select.tsx`. The second half didn't, for a non-
  primitive reason: **`source/admin-app/web/src/components/core/
  ui/sheet.tsx` is still in the tree** (Sheet was dropped from
  *design* in the Popover slice, but the file still exists
  pending the migrations of its three consumers — `EditDhcpConfigSheet`,
  `CreateReservationSheet`, `MobileMenu`), and `sheet.tsx`
  carries `data-open:animate-in / data-open:fade-in-0 / data-
  closed:animate-out / data-closed:fade-out-0 / data-[side=…]:
  data-open:slide-in-from-{bottom,left,right,top}-10 / …slide-out-to-…-10`
  inline. So tw-animate-css stays in `package.json` deps and
  the `@import "tw-animate-css"` stays in `index.css` until those
  three feature-slices replace `core/ui/sheet.tsx` with their
  inline-detail/edit equivalents (per the Sheet-drop plan from
  the Popover slice). **Last holdout: `sheet.tsx` (one file, four
  classes of utility inline).** The dependency-removal commit
  rides whichever of those three feature slices ends up being
  the last to migrate — with no primitive-level callers
  remaining, that slice closes the open question by deleting
  `core/ui/sheet.tsx` along with the dep.
- **Legacy shadcn alias audit — Select slice** (2026-05-09).
  The deleted `select.tsx` referenced `bg-popover`,
  `text-popover-foreground`, `bg-input`, `border-input`,
  `bg-accent`, `text-accent-foreground`, `text-muted-foreground`,
  `border-ring`, `ring-ring`, `border-destructive`,
  `ring-destructive`, `bg-border`, `ring-foreground/10`, plus
  tw-animate-css utilities. **Two alias rows are now down to one
  consumer each** — `bg-popover` (2→1, sole remaining consumer:
  `core/ui/command.tsx`) and `text-popover-foreground` (2→1,
  same file). The Command primitive port (cmdk-backed; post-
  tabs/forms slice) will drop both rows to zero, at which point
  the alias-pruning slice can delete those rows from
  `index.css`. Other Select-mentioned rows still have many
  consumers (the long tail of `bg-input` / `border-input` /
  `bg-accent` / etc. is broad) — no other rows newly zero-
  referenced. **Flagging for the alias-pruning slice:**
  `bg-popover` and `text-popover-foreground` are now one-
  consumer rows; the Command port will set them up for
  deletion.
- **Tabs — first non-floating Radix-wrap; surveyed five call sites,
  zero used `variant="line"` or `orientation="vertical"`** (2026-05-09,
  tenth primitive). Going-in load-bearing question was which of
  Forge's two tabs surfaces (`.tabs` pill / `.tabs-bar` underline)
  the primitive lands on. Survey of `core/ui/tabs.tsx` consumers
  (`DnsStatsSection`, `TunnelThroughputChart`, `CreateTunnelInline`,
  `pages/Devices`, `pages/Dhcp`) showed all five exclusively use
  the default pill — none pass `variant="line"`, none pass
  `orientation="vertical"`, and all five are *view switches inside
  a card or section*, never page-level navigation. So Decision 1
  collapsed to option (a) per the briefing: ship one primitive
  bound to one surface (`.tabs`). The legacy `variant="line"`
  branch and its parallel CVA was a feature with zero consumers —
  dropped per the locked rule "drop features the migration doesn't
  require." `.tabs-bar` itself doesn't even exist in `source/forge/
  styles.css` (only referenced in `source/forge/docs/tailwind.
  config.js`), so the slice doesn't add it; if a future router-
  driven page-nav surface needs an underline strip, that's a CSS
  + nav-component slice on its own (and it likely won't go through
  Radix Tabs at all — URL state is the source of truth, not React
  state). **Generalisation:** "primitive ships only the surface(s)
  consumers actually use; surfaces with zero consumers don't ship,
  even when Forge's docs imply they exist."
- **Tabs — `.tabs` already styles `button` descendants, so the
  Trigger is a passthrough, not a Forge-class-bearing part**
  (2026-05-09, tenth primitive). Forge's `.tabs button { … }`
  selector targets descendants directly (line 444 of styles.css);
  there's no `.tabs__btn` / `.tab` class for individual triggers.
  Radix's `Tabs.Trigger` defaults to a `<button>` element, so it
  drops naturally into the `.tabs button` selector with zero
  className needed on the Radix part. Three of four exports are
  passthrough Radix (`Tabs`, `TabsTrigger`, `TabsContent`) and
  only `TabsList` bears a Forge class (`.tabs`). **Generalisation:**
  when Forge's surface uses a descendant selector (`.parent child`)
  rather than a child class (`.parent__child`), the inner Radix
  parts become passthroughs — the parent's class plus the Radix
  default element is enough to land Forge's visuals. Same
  pattern would apply to any future Radix-wrap whose Forge surface
  styles by tag rather than by class.
- **Tabs — third application of the data-state bridge; static
  visual, no keyframes** (2026-05-09, tenth primitive). Toggle
  bridged `[data-state="checked"]` ↔ `.is-on`; Modal/Popover/
  DropdownMenu/Select bridged `[data-state="open"|"closed"]` and
  side-aware `[data-side="…"]` for floating surfaces. Tabs adds
  `[data-state="active"]` next to the existing `.tabs button.is-
  active` selector — same Toggle-pattern dual selector — so CSS-
  only mocks (using `.is-active`) and React primitives (Radix-
  emitted `data-state="active"`) land on the same visual. Unlike
  the floating-surface primitives, the active-state visual on
  `.tabs button` is a **static** transition (background + color +
  shadow swap, no animation declared), so no new keyframes —
  this confirms the Modal-slice intrinsic-vs-variant rule covers
  static state changes too: state-driven CSS lives in Forge
  whether or not the change is animated. **Generalisation:** the
  state-bridge selector family is universal — every state-bearing
  Radix-wrap grows it — but the Forge keyframe family is
  conditional on whether the surface animates the state at all.
- **Tabs — multi-part template's downward extreme so far**
  (2026-05-09, tenth primitive). Four exports across two shapes
  (3 passthrough Radix + 1 Forge-class-bearing). Comparison axis
  across the ten Radix-wrap primitives that have shipped:
  `Modal` 9/4 → `AlertModal` 8/4 → `DropdownMenu` 5/2 →
  `Select` 5/2 → `Tabs` 4/2 → `Toggle` 1/1 → `Popover` 3/2.
  Tabs sits between Toggle (single export) and Popover (3/2) on
  the small end. Confirms the template handles the floor of the
  range as cleanly as the ceiling — no new shape needed. Briefing
  prediction "4 exports across 1–2 shapes" landed exactly.
- **Tabs — Root passthrough is fine without a baked flex column
  default; call sites that need layout already pass it explicitly**
  (2026-05-09, tenth primitive). Legacy primitive's Root applied
  `flex gap-2 data-horizontal:flex-col` so the Root acted as a
  vertical flex column. New primitive's Root is a plain Radix
  passthrough (default `<div>`, no flex). Verified per call site:
  `Devices` and `Dhcp` already pass `className="flex min-h-0 flex-1
  flex-col"` for their fill-the-card layout (legacy default was
  redundant there); `CreateTunnelInline` has a TabsList followed
  by TabsContent panels — the panels are block-level so they
  break onto a new line below the inline-flex `.tabs`, with
  `mt-4` on each TabsContent providing the gap (legacy `gap-2`
  was ineffective here for the same reason — only one TabsContent
  renders at a time); `DnsStatsSection` and `TunnelThroughputChart`
  contain only a TabsList, so Root layout is irrelevant.
  **Generalisation:** when removing a default-className from a
  primitive Root, walk every call site and confirm either (a) the
  default was never load-bearing, or (b) the call site already
  supplies its own equivalent — leaving the default in place
  would propagate a feature with no consumers (same shape as the
  `variant="line"` drop earlier in this slice).
- **Legacy shadcn alias audit — Tabs slice** (2026-05-09). The
  deleted `tabs.tsx` referenced `bg-muted`, `text-muted-foreground`,
  `bg-foreground`, `bg-background`, `border-input`, `bg-input`,
  `text-foreground`, `border-ring`, `ring-ring`. None of the
  shadcn alias rows reached zero after deletion — these are
  long-tail rows with many remaining consumers (`text-muted-
  foreground` ~73 files, `bg-muted` ~19, etc.). The Command port
  remains the next slice that materially moves the alias-deletion
  needle (`bg-popover` 1→0, `text-popover-foreground` 1→0).
  **`tw-animate-css` holdout state unchanged** — `core/ui/sheet.
  tsx` is still the sole file carrying utility-class motion, and
  Tabs didn't touch it; dependency removal still rides whichever
  feature slice last migrates a Sheet consumer.
- **Form-row slice scoped down from "six primitives" to "three lights
  + Field composition"** (2026-05-09, eleventh slice). The briefing
  flagged Option B as Label / Input / Textarea / InputGroup / Ipv4Input
  / MacInput. Surveying first revealed two scope simplifications: (a)
  `core/ui/input-group.tsx` has **exactly one consumer**, `core/ui/
  command.tsx` — itself an out-of-scope holdout pending the Command
  port. So InputGroup naturally rides the Command port (where its
  sole consumer also lives) rather than this slice — same pattern as
  `core/ui/sheet.tsx` riding its three feature consumers' migrations.
  (b) `Ipv4Input` and `MacInput` are 150-line domain compositions
  (IPv4 octet + MAC hex segment validation, paste detection,
  auto-tab between segments). They use Forge's `.field input.mono`
  pattern visually but the segmented-input logic isn't a generic
  primitive — per the Card slice's "domain-coupled compositions stay
  in `compound/`" rule, they're not forge-web primitives. Restyling
  them in place (eliminating their shadcn-alias styling) is its own
  follow-up slice; this slice leaves them untouched. **Generalisation:**
  before designing for the briefing's stated scope, survey the
  consumer count and the litmus-test (pattern-reusable vs
  domain-coupled). The "form-row foundation" landed as Label / Input
  / Textarea + Field composition — five exports total in this slice;
  three remaining form primitives (InputGroup / Ipv4Input / MacInput)
  ride later slices with clearer scope.
- **Form lights — `.label` / `.input` / `.textarea` standalone
  classes added to Forge alongside the existing `.field` descendants;
  comma-selector dedupe** (2026-05-09, eleventh slice). Forge's `.field`
  block (lines 852–865 in `styles.css`) only had descendant selectors
  (`.field input`, `.field label`, etc.) — no standalone class for a
  free-standing input/label/textarea. Two options: (a) require every
  call site to wrap in `<div class="field">` so the descendant rules
  kick in (forces structural rewrites at 21+ call sites — out of scope
  for a "pure import-path retarget" slice); (b) add standalone classes
  (`.label` / `.input` / `.textarea`) that share rules with the
  descendant variants via comma selector. Picked (b). The refactor
  collapses the `.field` block from 4 selectors to 6 selectors but
  with shared declarations — `.input, .textarea, .field input, .field
  select, .field textarea { … }` — so a future Forge change to input
  visuals applies to both standalone and field-wrapped variants
  uniformly. Added: disabled state (`:disabled { cursor: not-allowed;
  opacity: 0.6 }`), placeholder color (`::placeholder { color: var(
  --ink-3) }`), textarea-specific `min-height: 64px` + `field-sizing:
  content` + `resize: vertical`. Did NOT add `[aria-invalid]`
  styling — surveying call sites showed zero consumers pass
  `aria-invalid` (the legacy primitive declared the styling but no
  consumer used it). Per "drop features the migration doesn't
  require." If a future consumer needs validation visuals, that's a
  Forge addition + primitive prop in its own slice. **Generalisation:**
  when a Forge surface only has descendant rules (`.parent child`)
  but the React primitive is a free-standing class-applier, add a
  comma-selector to share rules between standalone (`.child`) and
  descendant (`.parent child`) — this avoids duplicating the
  declarations and keeps both styling vocabularies aligned. Same
  pattern would apply to future surfaces (e.g., a future `.button`
  inside `.toolbar` if Forge ever ships `.toolbar button` rules).
- **Field — first composition primitive in `forge-web/`, locks the
  edit/read pattern as a single component** (2026-05-09, eleventh
  slice). Beyond the three lights, this slice ships a `Field`
  composition (`@wardnet/forge-web/field`) that wraps Label + control
  + optional help text and supports an `editing` boolean for
  edit-vs-read mode. API: `<Field label htmlFor help editing value>{
  control }</Field>`. When `editing=false` and `value` is provided,
  the children (input control) are replaced with a `<span class=
  "field-value">{value}</span>` styled to match the input's vertical
  rhythm so layout doesn't jump on mode swap. Forge gained two
  tiny additions: `.field-help` (small ink-3 text below the control)
  and `.field-value` (read-mode value display, 13px ink + 8px
  vertical padding to align with `.input`'s box height). **Naming
  follows the locked rule "component name follows Forge class":**
  Forge class is `.field` (single word), React export is `Field`
  (single word) — not `FormField` / `FormInput`. The single-word
  Forge vocabulary keeps consistency with `.modal` / `.popover` /
  `.toggle` / `.tabs` / `.card`; multi-word React names like
  `FormField` would re-introduce the same Radix-driven naming
  tension the Toggle slice closed. **Why this is a composition not
  a primitive:** Field combines three sub-elements (Label + control
  + help paragraph) and adds behavior (edit/read swap) on top of
  `.field`'s plain CSS structure — that's the same shape as `Card`
  + `CardHeader` + `CardBody` (the second slice's flat composition
  on top of `.card` + `.card__head` + `.card__body`). Card is the
  precedent; Field follows it. **Why this matters:** the manual
  pattern (`<div className="flex flex-col gap-2"><Label>X</Label>
  <Input/><p className="text-xs text-muted-foreground">Help</p></div>`)
  was reproduced verbatim across 15+ call sites. With Field, the
  pattern lives in one component — visual changes (gap, label
  weight, help text size) propagate by editing Forge or `field.tsx`
  rather than touching every consumer. Two call sites migrated to
  Field this slice as proof-of-pattern: `Login.tsx` (clean
  edit-only form, two label+input pairs) and `EditDhcpConfigSheet.
  tsx` (six fields, two with help text). Remaining ~13 call sites
  stay on the manual `<div>` + `<Label>` + `<Input>` pattern —
  they migrate to Field organically as feature slices touch them
  (or in a dedicated "field consolidation" slice). Both forms are
  visually equivalent so the codebase stays in a transitional but
  consistent state. **Generalisation:** when a manual structure is
  duplicated across enough consumers, ship a composition primitive
  even if it's "just a wrapper" — the value is design-system reach,
  not code reuse. Same logic justified Card.Header / Card.Body in
  the second slice.
- **Form-lights call site migration — pure import-path retarget,
  no JSX rewrite for the Label/Input/Textarea swap** (2026-05-09,
  eleventh slice). 21 call sites (Login, Step1Admin, Step4RouterMac,
  Dns, DnsLogs, DnsFilterProfile, DnsFilterProfileNew, MyDevice,
  BackupCard, CreateReservationSheet, DeviceDnsFilterCard,
  DeviceNetworkCard, DeviceSettingsCard, DnsFilterSettingsCard,
  EditDhcpConfigSheet, ManualTunnelTab, ProviderTunnelTab,
  UpdateCard, CronSchedulePicker, DhcpStatusCard, plus the
  `core/ui/input-group.tsx` internal imports) swapped via sed:
  `@/components/core/ui/{label,input,textarea}` → `@wardnet/
  forge-web/{label,input,textarea}`. Inline className overrides
  on `<Label>` (e.g., Login's `text-foreground/70`, DnsLogs's
  `text-xs text-muted-foreground`) are visually equivalent to or
  near-equivalent to Forge's `.label` defaults (12px ink-3 medium),
  so the Tailwind utility wins via cascade order with no behavior
  change — left in place. A follow-up "form polish" slice can
  drop the now-redundant inline classes. `core/ui/input-group.tsx`
  was updated to import Input/Textarea from forge-web (its sole
  consumer is `core/ui/command.tsx` which is out of scope; the
  InputGroup file stays put with its shadcn-alias internal styling
  pending the Command port). Legacy `core/ui/{label,input,textarea}.
  tsx` deleted. **Generalisation:** when Tailwind utilities and
  Forge classes apply to the same element via clsx, Tailwind v4's
  cascade order puts utility classes after component layers — so
  inline utility overrides win without needing `!important` or
  CSS specificity tricks. Useful for slices that want to ship the
  Forge primitive without forcing every consumer to drop their
  inline overrides at the same time.
- **Legacy shadcn alias audit — Form-lights slice** (2026-05-09).
  The deleted `label.tsx` / `input.tsx` / `textarea.tsx` referenced
  `border-input`, `bg-input/50`, `bg-input/30`, `bg-input/80`,
  `text-foreground` (file: prefix), `text-muted-foreground`,
  `border-ring`, `ring-ring`, `border-destructive`,
  `ring-destructive/20`, `ring-destructive/40`,
  `ring-destructive/50`, plus `data-[disabled=true]` /
  `peer-disabled:` utilities. None of the shadcn alias rows reached
  zero — `border-input`, `bg-input` family, and the `*-destructive`
  rows still have many remaining consumers (the form-heavies
  Ipv4Input/MacInput/InputGroup are unchanged this slice and keep
  consuming `border-input`/`ring-ring`/`text-muted-foreground`/etc.).
  Command port + form-heavies restyle slice are the next slices
  that will move the needle. **`tw-animate-css` holdout state
  unchanged** — sheet.tsx still the sole file carrying utility-
  class motion.
- **RadioGroup dropped — sole consumer collapsed into the existing
  `RoutingSelector` compound** (2026-05-09, twelfth slice). The
  briefing scoped this slice as "port RadioGroup as the fourth
  data-state-bridge application." Pre-flight survey turned up exactly
  one consumer: `pages/MyDevice.tsx`'s `RoutingForm`, where it
  rendered a binary "Direct (no VPN)" / "VPN" choice followed by a
  conditional `<Select>` listing tunnels. Walked back: that two-
  control composition (radio-binary then conditional dropdown) is
  the same affordance as the unified `<Select>` already used by the
  admin-side device page (`features/DeviceSettingsCard` → `compound/
  RoutingSelector`) where `Direct (no VPN)` is the first option,
  followed by tunnels — single dropdown, no separate binary toggle.
  RoutingForm was a leftover that pre-dated `RoutingSelector`.
  Replacing MyDevice's RoutingForm contents with a `<RoutingSelector
  />` instance collapses the three primitives (RadioGroup +
  RadioGroupItem + Select) down to one compound consumer call —
  and drops RadioGroup's app-wide consumer count to zero. **So the
  primitive ports out of scope: marked `[-]` in the table, legacy
  `core/ui/radio-group.tsx` deleted, no forge-web port written.**
  RoutingSelector's `tunnels` prop was widened from `Tunnel[]` to
  `TunnelSummary[]` — the compound only consumes `id` / `label` /
  `country_code` (the three fields TunnelSummary exposes), and
  `TunnelSummary` is the SDK's auth-scoped shape for self-service
  routing selection (the unauthenticated/self-service `/api/devices/
  me` endpoint deliberately ships only the minimum fields, not full
  Tunnel data — the full `Tunnel` shape carries internal stats like
  `bytes_tx`/`endpoint`/`last_handshake` that the self-service
  context shouldn't see). Narrowing the prop also makes the actual
  data dependency explicit. The two existing `RoutingSelector`
  consumers (`DeviceSettingsCard`, `Step6Policy`) keep working
  unchanged because `Tunnel` is structurally a `TunnelSummary` —
  TS structural assignability lets `Tunnel[]` flow into a
  `TunnelSummary[]` prop. **Generalisations:**
  (a) "Drop features the migration doesn't require" extends one
  more rung — when a primitive's lone consumer can be expressed
  with an existing primitive or compound, the primitive itself
  drops, not just its unused parts. The Forge-vocabulary thresh-
  hold for shipping a primitive is "≥1 consumer that can't be
  expressed with what we already have," not "≥1 consumer."
  (b) When a compound takes a domain type where only a structural
  subset is consumed, prefer the narrower shape (`TunnelSummary`,
  not `Tunnel`) — declares the actual data dependency and matches
  whichever auth/scoping boundary the consumer sits inside. Same
  litmus would have applied if the SDK had a `DeviceMin` /
  `DeviceFull` distinction.
  (c) Pre-flight surveys can change the slice's shape from "port"
  to "drop" without writing a primitive at all — the briefing's
  Option A becoming "delete the primitive, refactor the consumer"
  saves both Forge growth (no `.radio` / `.radio-group` classes
  added) and forge-web surface (no new primitive file, no new
  subpath export). The data-state-bridge "fourth application"
  lands as a *deferred* generalisation the next state-bearing
  primitive will surface; nothing was lost by skipping it here.
- **MyDevice — RoutingForm shrunk from 60 lines to 30 by re-using
  the compound** (2026-05-09, twelfth slice). The pre-existing
  RoutingForm in `pages/MyDevice.tsx` carried 60 lines of state
  (separate `mode` and `tunnelId` `useState`s reconstructing the
  `RoutingTarget` shape on every render), JSX (RadioGroup +
  conditional Select + empty-tunnels message), and helper logic
  (`initialMode` / `initialTunnelId` derivation). `RoutingSelector`
  already encapsulates all of that — value/onChange flows full
  `RoutingTarget`s, internal state derives the dropdown value, and
  the empty-tunnels case is rendered as a disabled-Select-plus-
  message. Replacing the form contents with `<RoutingSelector
  value={target} onChange={setTarget} tunnels={tunnels} />` cut
  RoutingForm to a single `useState<RoutingTarget>`, the
  RoutingSelector instance, an error alert, and a Save button —
  ~30 lines from ~60. **Generalisation:** when a feature page
  reimplements a domain-coupled UI pattern that already exists as
  a compound, prefer collapsing the page's bespoke form into the
  compound over preserving the page-local shape — same shape as
  the AlertModal slice's "primitives that wrap Radix parts whose
  default rendering is button-like do NOT bake a `<Button>`
  wrapper" rule, applied at the compound level. The cost is one
  extra prop (`tunnels` flowing through) and one extra
  `useState<RoutingTarget>`; the benefit is one compound owning
  the routing-selection visual + behavior across all consumers.
- **Legacy shadcn alias audit — RadioGroup-drop slice** (2026-05-09).
  The deleted `radio-group.tsx` referenced `border-input`,
  `border-ring`, `ring-ring`, `bg-input/30`, `border-destructive`,
  `ring-destructive/20`, `ring-destructive/40`, `ring-destructive/
  50`, `border-primary`, `bg-primary`, `text-primary-foreground`,
  `bg-primary-foreground`. None of the shadcn alias rows reached
  zero — these are long-tail rows with many remaining consumers
  (Ipv4Input/MacInput/InputGroup keep `border-input`/`ring-ring`/
  etc.; Button continues to source `bg-primary`/`text-primary-
  foreground` via CVA mapping). The MyDevice change additionally
  removed three RadioGroup imports and one Label import (the now-
  unused `<Label htmlFor="routing-direct">` / `<Label htmlFor=
  "routing-vpn">` paired with each RadioGroupItem) — neither row
  changed count materially. **`tw-animate-css` holdout state
  unchanged** — sheet.tsx still the sole carrier.
- **Command renamed to Combobox — the cmdk wrapper is a filterable
  select, not a command palette** (2026-05-09, thirteenth slice).
  The legacy `core/ui/command.tsx` was the standard shadcn 9-export
  template (Command / CommandDialog / CommandInput / CommandList /
  CommandEmpty / CommandGroup / CommandItem / CommandShortcut /
  CommandSeparator) wrapping cmdk. Pre-flight survey: exactly **one
  app consumer**, `compound/CountryCombobox.tsx`, which uses cmdk
  inside a Popover as a typeahead country picker. Three exports
  (`CommandDialog`, `CommandShortcut`, `CommandSeparator`) had zero
  consumers. There is no global ⌘K palette, no app-wide command
  runner — cmdk exists in this codebase solely for type-to-filter
  inside a popover. So the primitive's *shadcn name* (`Command`,
  evoking ⌘K palettes) misrepresented its actual role. Renamed to
  `Combobox` — the standards-compliant ARIA name for "select with a
  search input," and the noun the consumer file already uses
  (`CountryCombobox`). The rename also closes a naming-rule
  inconsistency the slice was about to introduce: shadcn-style
  multi-part exports (CommandInput / CommandList / CommandEmpty /
  CommandItem) would have crowded forge-web's vocabulary with
  Command-prefixed parts no consumer ever asked for. **Generalisation:**
  when the underlying library's name describes its *full generic
  capability* but the codebase only consumes a narrow sub-pattern,
  name the primitive after the *sub-pattern* (Combobox), not the
  library (Command/cmdk). Same shape as the AlertModal slice's
  decision to name after behavior (Alert prefix) rather than the
  Radix sub-library — but applied at the granularity of "what
  this codebase actually uses cmdk for." The cmdk dependency moved
  from `admin-app/web` to `admin-app/forge-web` — the React
  primitives layer owns the cmdk wrapper, the app no longer depends
  on cmdk directly.
- **Combobox — high-level composite, not a multi-part slot** (2026-05-09,
  thirteenth slice). Two architectural shapes considered: (a) keep
  the multi-part shadcn shape and rename Command* → Combobox*
  (consumer wires `<Popover><PopoverTrigger><Button/></PopoverTrigger>
  <PopoverContent><Combobox><ComboboxInput/><ComboboxList><ComboboxEmpty/>
  <ComboboxGroup>{items.map(c => <ComboboxItem/>)}</ComboboxGroup>
  </ComboboxList></Combobox></PopoverContent></Popover>` — eight
  layers per consumer); (b) high-level composite that owns Popover
  + trigger Button + search input + list scaffold + empty state +
  chevron + selected-checkmark indicator, with a single children
  slot for items. Chose (b). API: `<Combobox value onChange trigger
  searchPlaceholder empty disabled>{items.map(o => <ComboboxItem
  value keywords>...</ComboboxItem>)}</Combobox>` — two exports
  (Combobox + ComboboxItem) instead of the multi-part six. **Why
  composite:** the two-component shape lets the composite encapsulate
  the cmdk machinery while still letting consumers control item
  content (children of `<ComboboxItem>` is anything — flag emoji +
  name, multi-line, badges, …). Consumer also controls the trigger
  *content* via the `trigger` prop (selected-label vs placeholder
  is the consumer's `?:`); the composite owns the trigger Button
  itself (variant=outline, full-width, role=combobox, chevron). The
  selected-checkmark indicator is owned by Forge — `<ComboboxItem>`
  emits `data-state={isSelected ? "checked" : "unchecked"}`, and
  the existing `.menu-item[data-state="checked"]::after` rule
  (added in the Select slice) paints the check via mask-image.
  **Generalisation:** when designing a primitive whose underlying
  library has a multi-part API (cmdk's Command*, Radix's Dialog*),
  evaluate two routes: (1) thin pass-through wrap each part as
  separate exports (Modal / AlertModal / DropdownMenu / Select all
  followed this — they're slot-style multi-part composites that
  look like the underlying library); (2) high-level composite when
  the codebase's usage pattern is *narrower* than the library's
  full capability and the consumer-side complexity is high (Combobox
  picked this — eight layers became two). Modal-and-friends couldn't
  go (2) because Dialog is genuinely multi-purpose (header / body /
  footer / scrollable / different layouts). cmdk is also multi-
  purpose (palettes / runners / search) but our usage is one-purpose,
  so (2) is justified. **The decision is consumer-driven:** what is
  the codebase actually doing with this library?
- **Forge — `.combobox-trigger` / `.combobox-input` / `.combobox-list`
  / `.combobox-empty` added; items reuse `.menu-item` via
  `data-selected` bridge** (2026-05-09, thirteenth slice). cmdk's
  `<Command.Item>` emits `data-selected="true"` on the keyboard-
  focused item — same affordance as Radix DropdownMenu's
  `data-highlighted` (the keyboard-focus-visible row). Bridged
  via comma selector: `.menu-item[data-highlighted], .menu-item
  [data-selected="true"] { background: var(--bg-sunken); … }` —
  one rule, two attribute vocabularies, same visual. This is the
  third application of the comma-selector dedupe pattern (after
  `.label`/`.field label` and `.input`/`.field input` in the
  form-row slice) — confirms the pattern generalises beyond
  "standalone vs descendant" to "different libraries' state-
  attribute vocabularies for the same affordance." `.combobox-
  trigger` styles the outline-Button trigger to be full-width with
  a chevron; `.combobox-content` overrides `.popover`'s default
  padding (zero — the search input has its own border-bottom
  delimiter) and matches trigger width via Radix's
  `--radix-popover-trigger-width` var; `.combobox-input` is a
  flex-row with leading search icon + bare-input child;
  `.combobox-list` is a 280px-max scrollable list region;
  `.combobox-empty` is centered ink-3 text. **Generalisation:**
  bridges between attribute vocabularies (Radix `data-highlighted`
  ↔ cmdk `data-selected="true"`) live in CSS via comma selector,
  not in JS via attribute mapping. Consistent with the existing
  rule "Radix `data-state` bridged in CSS, not JS."
- **InputGroup dropped — second consecutive slice where the
  briefing's "port primitive X" flipped to "drop primitive X"**
  (2026-05-09, thirteenth slice). InputGroup's only purpose in
  this codebase was being the wrapper used by `core/ui/command.
  tsx`'s `CommandInput` (search input + leading icon). The new
  `Combobox` composite renders its search input via a Forge
  `.combobox-input` class instead — small flex-row with a
  search-icon child and a bare cmdk Input. With Command gone and
  no other consumers, InputGroup's app-wide consumer count drops
  to zero, so the primitive itself drops per the same "primitive
  itself drops" rule the RadioGroup slice locked. Legacy `core/
  ui/input-group.tsx` deleted; no forge-web port written. This is
  now a pattern: **across slices 12 and 13, two of the three
  briefing-scoped Radix-or-equivalent ports flipped from "port"
  to "drop" after pre-flight survey** (RadioGroup, InputGroup).
  Both saved Forge growth, both narrowed forge-web's surface,
  both are validations of the "≥1 consumer that can't be
  expressed with what we already have" threshold. The
  pre-flight-survey-flips-slice-shape rule has now been applied
  three slices in a row counting Command-rename → Combobox as a
  third instance of "the briefing's primitive name was wrong; the
  pre-flight reframed the slice."
- **`bg-popover` + `text-popover-foreground` deleted from
  `index.css` — first alias rows to reach zero** (2026-05-09,
  thirteenth slice). Per the seventh / eighth / ninth slice audits
  (`bg-popover` 7→6→5→2→1, `text-popover-foreground` 5→4→3→2→1),
  this slice's deletion of `core/ui/command.tsx` zeroed both rows.
  Bundled their deletion from `admin-app/web/src/index.css` (lines
  89–90) into this commit since the deletion is two trivial line
  removals and the alias-pruning slice would otherwise just delete
  these same two lines. **Generalisation:** when an alias-row's
  consumer count reaches zero in the same slice that removes the
  last consumer, bundle the row deletion in that slice — splitting
  is unnecessarily ceremonial. Rows that reach zero across multiple
  slices (the long-tail rows like `text-muted-foreground`,
  `bg-muted`, `border-input`) still wait for the alias-pruning
  sweep where they'll be deleted en masse. **Legacy shadcn alias
  audit — Combobox slice:** the deleted `command.tsx` referenced
  `bg-popover` (1→0), `text-popover-foreground` (1→0), `bg-border`
  (in `bg-border`, kept), `text-foreground` (kept), `bg-muted` (in
  `data-selected:bg-muted`, kept), `text-muted-foreground` (kept);
  the deleted `input-group.tsx` referenced `border-input`,
  `bg-input/50`, `bg-input/30`, `bg-input/80`, `text-muted-
  foreground`, `border-ring`, `ring-ring`, `border-destructive`,
  `ring-destructive/20`, `ring-destructive/40` — all long-tail
  rows with remaining consumers (the form-heavies Ipv4Input/
  MacInput keep `border-input`/`ring-ring`). **`tw-animate-css`
  holdout state unchanged** — sheet.tsx still the sole file
  carrying utility-class motion.
- **Field consolidation slice — Field is the only form-row
  primitive in the app; Label is encapsulated inside Field and
  no longer imported by any consumer** (2026-05-09, fourteenth
  slice). Sweep slice that picked up the eleventh slice's
  unfinished work. Migrated every `<Label>` + control pair
  across `pages/`, `components/features/`, and `components/
  compound/` to `<Field>`. After the sweep, `grep -rn '<Label\b'
  source/admin-app/web/src/` returns zero JSX matches and the
  Label primitive's only remaining consumer is `forge-web/src/
  primitives/field.tsx` itself. Files touched: `pages/setup/
  Step1Admin.tsx`, `pages/setup/Step4RouterMac.tsx`, `pages/
  Login.tsx` (already migrated in slice 11), `pages/Dns.tsx`,
  `pages/DnsLogs.tsx`, `pages/DnsFilterProfile.tsx`, `pages/
  DnsFilterProfileNew.tsx`, `components/features/{BackupCard,
  CreateReservationSheet, DeviceDnsFilterCard, DeviceNetworkCard,
  DeviceSettingsCard, DnsFilterSettingsCard, ManualTunnelTab,
  ProviderTunnelTab, UpdateCard}.tsx`, `components/compound/
  {DhcpStatusCard, CronSchedulePicker}.tsx`. **Generalisation:**
  consolidation slices are valuable when an earlier slice
  introduced a primitive as proof-of-pattern (slice 11 shipped
  Field with two consumers as proof). The "half-adopted
  primitive" state (Field exists but most call sites still use
  the manual structure) is technical debt — every slice after
  the introduction either ships more consumers or accepts the
  primitive as decorative. This slice resolved that for Field.
- **Field gained `direction="row"` and `labelId` to absorb every
  remaining label+control pattern in the codebase** (2026-05-09,
  fourteenth slice). Beyond the vertical Label-above-control
  pattern Field shipped with, the sweep surfaced two more
  patterns Field had to support to truly become "the only
  component used on forms":
  (a) **Horizontal settings rows** — label-and-help block on
  the left, toggle / select on the right (`.flex.items-center.
  justify-between`-style rows). Examples: DnsFilterSettingsCard's
  "DNS filtering enabled" + description + Toggle, DhcpStatusCard's
  "Enable DHCP" + Toggle, UpdateCard's "Channel" + Select,
  Dns.tsx's "Enable DNS" + Toggle, DeviceDnsFilterCard's "DNS
  filtering" + description + Toggle. Added `direction="column" |
  "row"` prop. When `direction="row"`, Field renders as
  `.field[data-direction="row"]` with `flex-direction: row;
  align-items: center; justify-content: space-between` and wraps
  label + help into a `.field-text` block on the left so the
  description sits under the label naturally. Vertical (default)
  behaviour unchanged. **Why a data-attribute, not a `--row`
  modifier class:** consistent with the locked rule "data-
  attribute > modifier-class for prop-driven variants"; the
  attribute reflects the prop directly so React vocabulary
  (`direction="row"`) maps 1:1 to CSS selector
  (`[data-direction="row"]`).
  (b) **`aria-labelledby`-style controls** — `ProfileToggleList`'s
  custom widget consumes `ariaLabelledBy` instead of pairing
  with `htmlFor`, so its label needs `id` rather than `htmlFor`.
  Added `labelId?: string` to Field; passed through to the
  internal `Label` as `id`. Two consumers (DnsFilterSettingsCard,
  DeviceDnsFilterCard). **Generalisation:** when a primitive is
  designed for "the canonical case" (label `htmlFor` ↔ control
  `id`), surfacing the alternative-association case (`aria-
  labelledby` ↔ label `id`) as a sibling prop is cleaner than
  forcing consumers to drop back to the manual structure or
  exposing the underlying Label primitive. The Field API stays
  cohesive ("a labeled control") while supporting both
  association strategies.
- **Toggle-then-Label rows flipped to Label-then-Toggle when
  migrating** (2026-05-09, fourteenth slice). Two call sites
  (UpdateCard's "Automatically install when available" toggle,
  DnsLogs's "Live tail" toggle) used the control-then-label
  pattern (`<Toggle/> <Label/>`) where the toggle visually
  precedes its caption. Field renders `[label, control]` —
  migrating these flipped the visual order to `[label, toggle]`.
  The visual change is minor (label reads naturally on either
  side of a toggle) and the consistency win — every form/setting
  row has its label on the same side — outweighs preserving
  the toggle-first idiom for two cases. **Generalisation:** when
  a primitive enforces a single layout convention, consumers in
  the inverse layout flip to match the convention rather than
  the primitive growing a `controlPosition` prop for the long
  tail. The threshold for adding the prop would be enough
  consumers (or strong enough domain meaning) to justify a
  divergent convention; two cases of cosmetic difference don't
  clear it.
- **Help text placement — Field renders help below the control,
  but contextual hints can sit above by passing them as part of
  `children`** (2026-05-09, fourteenth slice). Field's `help`
  prop renders a `<p class="field-help">` *after* the children
  — the standard "input + help" reading order. Two call sites
  (DeviceDnsFilterCard's `<DefaultProfileHint>` above
  `<ProfileToggleList>`, DnsFilterSettingsCard's "Applied to
  devices..." paragraph above `<ProfileToggleList>`) wanted
  the hint *between* label and control instead. Solution: keep
  the hint inside `children` (alongside the control), don't
  pass `help`. Field's `.field` `gap: 6px` then handles the
  vertical spacing. **Generalisation:** Field's slot semantics
  are `[label, children, help]`. "Help below" maps to `help`;
  "help above" lives in `children` so the consumer keeps
  ordering control without Field needing a `helpPosition` prop.
  Same logic as the Toggle-then-Label decision — a primitive
  shouldn't grow positional knobs for layout variations a
  consumer can express in slot composition.
- **Sheet-to-inline migrations are mechanical once Field is the
  form-row primitive — the rewrite is wrapper-only, no per-row
  JSX work** (2026-05-09, fifteenth slice). EditDhcpConfigSheet
  folded into `compound/DhcpConfigCard` as an edit-mode (the
  DeviceSettingsCard precedent: `editing` state, Edit button in
  CardHeader's CardAction, Cancel/Save buttons in CardFooter,
  hook-coupled via `useUpdateDhcpConfig`); CreateReservationSheet
  renamed `features/CreateReservationInline.tsx` and rendered
  above the reservations table when open (the CreateTunnelInline
  / Tunnels.tsx precedent). Both form bodies copied verbatim from
  the sheet — every Field-wrapped row from slice 14 needed zero
  edits, only the chrome (Sheet → Card) changed. **Generalisation:**
  consolidating form rows into Field (slice 14) was the prerequisite
  that made these sheet-to-inline migrations near-mechanical. The
  same migration before slice 14 would have meant per-row Label /
  Input rewrites alongside the Sheet → Card rewrite — confirms
  the value of consolidation slices as scaffolding for downstream
  feature-shape changes.
- **Compound vs features dir is loose for cards that import
  hooks** (2026-05-09, fifteenth slice). Going-in question on
  A1: should DhcpConfigCard move from `compound/` to `features/`
  once it imports `useUpdateDhcpConfig` directly (matching
  DeviceSettingsCard's location)? Survey answer: `compound/`
  already has multiple hook-importing files (`RecentErrorsCard`,
  `TunnelCard`, `ConnectionBanner`, `Sidebar`,
  `UncleanShutdownBanner`), so "imports a domain hook" isn't the
  features/ litmus. Locked: **features/ is for whole-feature
  flows** (BackupCard's two-step export/restore, DeviceSettingsCard's
  device-edit composition, CreateReservationInline's create flow,
  CreateTunnelInline's manual-vs-provider create flow). compound/
  is for cards that present or edit *one* domain object even when
  they own the mutation hook. Result: DhcpConfigCard stays in
  `compound/`; CreateReservationInline lands in `features/`.
  Avoids a noisy file move and keeps the dir convention legible.
- **`onAdd?: () => void` (optional) on tables to hide both the
  in-table button and the empty-state action when undefined**
  (2026-05-09, fifteenth slice). DhcpReservationTable's
  `onAdd: () => void` (required) became `onAdd?: () => void`
  (optional) so the Dhcp page can pass `undefined` while the
  inline create card is already showing. Same shape as
  TunnelGrid's `onAdd?: () => void` (used by Tunnels.tsx:
  `onAdd={creating ? undefined : () => setCreating(true)}`).
  EmptyStatePlaceholder already gates its action button on
  `actionLabel && onAction`, so the table just forwards
  `actionLabel={onAdd ? "Add reservation" : undefined}` to it
  and wraps the in-table top-right button in `{onAdd && …}`.
  **Generalisation:** for tables that own both a top-right "Add"
  button and an empty-state CTA, an optional `onAdd` is the
  cleanest way to let the consumer hide *both* affordances for
  states where the create flow is already showing elsewhere on
  the page.
- **Tabs go from uncontrolled to controlled when one tab triggers
  state on another tab** (2026-05-09, fifteenth slice). The Dhcp
  page's "Make static" action lives in the Leases tab but opens
  a CreateReservationInline that lives above the Reservations
  table, so clicking it must (a) switch tabs and (b) open the
  inline form pre-filled. That requires `value` + `onValueChange`
  on the Tabs root — uncontrolled `defaultValue="leases"` can't
  flip the active tab from outside the Tabs root. Single
  `openReservationCreate(defaults?)` helper on the page batches
  both state changes (`setTab("reservations")` +
  `setReservationCreate({open: true, defaults})`). **Generalisation:**
  Tabs default to uncontrolled (the simplest API) but switch to
  controlled when an action on tab A drives state on tab B. No
  ceremony needed — Tabs primitive already supports both modes;
  just lift `value`/`onValueChange` to the page when needed.
- **`tw-animate-css` removal now blocked by `MobileMenu` only**
  (2026-05-09, fifteenth slice). Slice 9 (Select port) noted the
  removal was blocked by `core/ui/sheet.tsx` and its three
  consumers (EditDhcpConfigSheet, CreateReservationSheet,
  MobileMenu). This slice migrated the two form consumers; only
  MobileMenu remains. After A3 (mobile-nav redesign), `sheet.tsx`
  deletes, the `tw-animate-css` package drops from
  `admin-app/web/package.json`, and the `@import "tw-animate-css"`
  line drops from `index.css` — likely a one-commit follow-up to
  the MobileMenu slice. Confirms the slice 7 prediction
  ("`tw-animate-css` removal lands when whichever feature slice
  last touches Sheet replaces it") and locks the removal as a
  trailing edge of the MobileMenu slice rather than its own
  alias-pruning slice.

---

## Forge as source of truth (going forward)

This migration isn't "consume Forge once and forget." Forge becomes the
maintained design-system artifact: every visual decision flows through it
first, then into the apps.

### Rules

1. **Forge first, app second.** A new primitive, token, or pattern needed by
   `web-ui` or `site` is added to Forge in the same commit (or a preceding
   commit) that uses it. Mocks (`design-system.html`, `screens.jsx`,
   `detail-screens.jsx`, `mobile.html`) get the new component too — that's
   how Forge stays a working visual reference, not a stale museum.
2. **Skill stays in sync.** `.agents/skills/using-design-system/SKILL.md`
   gets updated alongside Forge changes — new primitives listed, new patterns
   documented.
3. **No app-only patterns.** If you find yourself reaching for a pattern that
   isn't in Forge, the right move is to lift it into Forge first — a token,
   a class, or a documented pattern in `design-system.html` — then use it.
   "I'll do it locally and port it later" is how design systems rot.
4. **Tailwind config is generated, not authored.** Forge's
   `tailwind.config.js` exists for Tailwind 3 / cdn-mode reference. Our
   apps use Tailwind 4's `@theme inline` in `index.css` — that block is
   the canonical mapping and should match `tailwind.config.js` token-for-token.
   Update both when tokens change.
5. **Tokens are the canonical source.** `@wardnet/forge/src/tokens.ts`
   (post-platform-split) is the authoritative TS object. `forge-web`'s
   `styles.css` is the CSS-var manifestation of those values; `forge-native`
   reads them directly into `StyleSheet`. When a token changes, change it
   in `tokens.ts` first and the rest follows.
6. **No platform leakage between Forge packages.** `forge` imports nothing
   platform-specific. `forge-web` may import `radix-ui` and emit CSS;
   `forge-native` may import `react-native` and emit `StyleSheet`. Neither
   imports the other. App-level code chooses its platform package.

### Forge update checklist (running list)

| Item                                                                          | Status |
| ----------------------------------------------------------------------------- | ------ |
| Rename `.agents/skills/using-desing-system/` → `using-design-system/`         | [x]    |
| Strip `[data-density="compact"]` block from `styles.css` (we ship comfortable only) | [x] |
| Strip non-floating sidebar variants from `styles.css` + collapse `.app[data-rail="floating"]` into `.app` | [x] |
| Decide on import strategy: vendor `styles.css` into each app, or import from `design-system/` via Vite alias (preferred — single source) | [x] |
| Document Radix-binding patterns in `design-system.html` §05 (e.g. how Switch ties to `.toggle`, how Dialog ties to `.modal`) — added Radix-binding sub-section in §05 on 2026-05-10 covering Switch/Dialog/AlertDialog/Drawer/DropdownMenu/Popover/Select/Tabs/Combobox/Label, plus four binding rules (class-on-Radix-node, state-in-DOM, scrim usage, Radix CSS vars); `.tbl`/`.stat` flagged as "Radix wrapper TBD". | [x] |
| Add Tailwind 4 `@theme inline` reference snippet in `README.md` so future apps know how to consume Forge tokens | [x] |
| Drop `tailwind.config.js` once Tailwind 4 reference is in README, OR keep as a Tailwind-3-compat reference (decide on first use) — decided drop on 2026-05-10 (no build consumes it; TW4 `@theme inline` reference already in `source/forge/docs/README.md` and `source/admin-app/web/src/index.css`) | [x] |
| Add any new primitives we introduce in the apps back into `primitives.jsx` (StatTile already there; new ones go here too) | [ ] |
| Update `design-system.html` §05 Components when we add new components in code | [ ]    |
| Bootstrap `source/forge/` workspace package (first primitive slice — see "Where Forge lives" below) | [x] |
| Move `styles.css` from `design-system/` to `source/forge/` once package exists; retarget `@wardnet/forge` alias | [x] |
| **Restructure: context-per-source-dir + admin-app internal workspace.** Rename `source/web-ui/` → `source/admin-app/web/`; move `source/forge/` (Radix primitives) → `source/admin-app/forge-web/`; rename `source/site/` → `source/marketing-site/`; move repo-root `design-system/` → `source/forge/docs/`. Create new top-level `source/forge/` (platform-neutral) with `tokens.ts` + `styles.css` + exports map. Create `source/admin-app/package.json` (workspaces: web, forge-web). Flip 44 button imports to `@wardnet/forge-web/button`. Update Makefile, CI, gitignore, daemon rust-embed paths. (See "Where Forge lives — context-per-source-dir + admin-app workspace" below.) | [x] |
| **Reserve `source/admin-app/forge-native/` and `source/admin-app/mobile/`** for the future React Native primitives + mobile bundle. No code in this branch — placeholder rule that the names are taken. | [x] |
| Complete `tokens.ts` extraction — initial slice covered brand / status / radius / density / font; surfaces (`--bg`, `--bg-elev`, …), ink (`--ink`, `--ink-2`, …), sidebar (`--side-*`), shadows, and soft-variant pairs still need transcribing. Until then `styles.css` is authoritative for web rendering. | [ ] |
| Convert `source/forge/docs/` to docs-only (mocks + studio HTML rendered against the real package builds long-term) — README clarified docs-only status on 2026-05-10; design-system.html already links canonical `../styles.css` (no embedded token block to drift); long-term studio-against-real-builds intent stated in README. | [x] |
| Delete legacy shadcn-token alias block in `source/admin-app/web/src/index.css` once no component references the old utilities (`bg-background`, `text-foreground`, `bg-primary`, `border-border`, `border-input`, `ring-ring`, `bg-sidebar*`, `bg-destructive`, `bg-muted`, `bg-success`, `bg-warning`, `bg-popover`, `bg-secondary`, `text-muted-foreground`, etc.) | [x] |
| Delete legacy `--brand-indigo` / `--brand-slate` / `--brand-green` / `--brand-green-hover` aliases in `source/marketing-site/src/index.css` once site components consume Forge tokens (`var(--accent)`, `bg-accent`, etc.) | [x] |

---

## Where Forge lives — context-per-source-dir + admin-app workspace (locked 2026-05-09; revised again 2026-05-09)

> **Status:** implemented. The previously-locked "root yarn workspace" plan
> was reconsidered before it shipped — it would have flattened the
> deployment-unit segregation that already organises `source/`. Final shape
> is described below.

### The organising principle

`source/` is **context-per-source-dir** — every top-level entry is a
deployment unit / release context with its own runtime and cadence:

| Dir                                | What ships                                | Cadence                          |
| ---------------------------------- | ----------------------------------------- | -------------------------------- |
| `source/daemon/`                   | The Rust daemon binary                    | CalVer release tags              |
| `source/forge/`                    | The Wardnet design language               | Versioned with the design system |
| `source/sdk/wardnet-js/`           | `@wardnet/js` — published TypeScript SDK  | Independent (eventually npm)     |
| `source/marketing-site/`           | Static public marketing site              | Continuous deploy                |
| `source/admin-app/`                | Admin product (web today, mobile later)   | Tied to daemon release           |
| `source/end2end-tests/daemon/`     | Container-based e2e harness               | Internal, not shipped            |

Forge being top-level matches this rule: it's the design language consumed
by both `admin-app/web` AND `marketing-site`. It is not an admin-product
internal.

### End-state layout

```
source/
  daemon/                       ← Rust daemon (unchanged)
  forge/                        ← @wardnet/forge — platform-NEUTRAL
                                  styles.css         ← Forge CSS (web manifestation of tokens)
                                  src/tokens.ts      ← canonical token VALUES (TS)
                                  src/types.ts       ← shared component-API contracts (later)
                                  src/voice.ts       ← banned words, principle strings (later)
                                  docs/              ← visual studio (was repo-root design-system/)
  sdk/wardnet-js/               ← @wardnet/js — published SDK, standalone yarn project
  marketing-site/               ← public site, standalone yarn project
                                  Consumes @wardnet/forge via portal:.
  admin-app/                    ← admin product
    package.json                ← workspaces: ["forge-web", "web", (later) "forge-native", "mobile"]
    .yarnrc.yml
    yarn.lock                   ← single lockfile for everything inside admin-app
    forge-web/                  ← @wardnet/forge-web — Radix + Forge classes (React primitives)
                                  Depends on: @wardnet/forge (portal:../../forge), radix-ui, react.
    web/                        ← @wardnet/admin-web — the admin web bundle
                                  (formerly source/web-ui).
                                  Depends on: @wardnet/forge-web (workspace:^),
                                              @wardnet/forge (portal:../../forge),
                                              @wardnet/js   (portal:../../sdk/wardnet-js).
    forge-native/               ← (future) @wardnet/forge-native — RN primitives
    mobile/                     ← (future) @wardnet/admin-mobile — admin mobile bundle
  end2end-tests/                ← unchanged, standalone yarn
```

### Why this shape, not "root workspace covering everything"

Three points:

1. **Context-per-source-dir is already the rule.** `source/<thing>/` already
   reads as "this is a deployment unit." Hoisting `forge` / `forge-web` /
   `forge-native` to siblings of `daemon` and `marketing-site` would muddy
   the rule: forge-web isn't a deployment unit, it's an admin-product
   internal. Keeping the workspace at admin-product scope keeps the rule
   intact — `source/admin-app/` is "the admin product"; what's inside is
   private to it.
2. **Different release cadences map cleanly to different yarn projects.** The
   SDK has its own publish cadence (eventually npm). The daemon has CalVer
   tags. The marketing site deploys continuously. Forging them all into one
   workspace would force a single `yarn install` and a single dep graph
   across release units that move at different speeds. Standalone yarn
   projects per top-level context preserve that independence.
3. **Mobile arrives at a known place.** When the React Native admin app
   spins up, it lands at `source/admin-app/mobile/` next to the existing
   `web/`, sharing `forge-native/` via `workspace:^`. No structural
   rewrite — just two new workspace members.

### Why `source/admin-app/web/` and not just `source/admin-app/`

Today the admin app is web-only; it would feel natural to have admin-app
*be* the web bundle. But the moment mobile lands the directory has to
either (a) split mid-stream, breaking history, or (b) get a `mobile/`
subfolder while web sits awkwardly at the package root. Putting `web/`
inside `admin-app/` from day one matches what mobile + native primitives
will look like once they exist, and makes the "admin product, web build"
mental model explicit.

### Imports

- **`source/admin-app/web`** imports primitives from
  `@wardnet/forge-web/*` and tokens / types from `@wardnet/forge/*` (and
  the SDK from `@wardnet/js`). Apps **never** import `radix-ui` directly
  — primitives wrap it.
- **`source/marketing-site`** imports only `@wardnet/forge/styles.css` and
  (future) `@wardnet/forge/tokens`. It does **not** depend on
  `forge-web` and never pulls Radix into its bundle.
- **`source/admin-app/forge-web`** imports from `@wardnet/forge` for
  shared tokens / types.
- **`source/admin-app/mobile`** (future) imports from
  `@wardnet/forge-native/*` and `@wardnet/forge/tokens`.
- **No cross-platform leakage.** `forge-web` may not import
  `react-native`; `forge-native` may not import `radix-ui` or
  `styles.css`. Only `@wardnet/forge` sits in both import graphs.

### Cross-package dependency protocols

| From                          | To                       | Protocol                       |
| ----------------------------- | ------------------------ | ------------------------------ |
| admin-app/web                 | admin-app/forge-web      | `workspace:^`                  |
| admin-app/web                 | top-level forge          | `portal:../../forge`           |
| admin-app/web                 | sdk/wardnet-js           | `portal:../../sdk/wardnet-js`  |
| admin-app/forge-web           | top-level forge          | `portal:../../forge`           |
| marketing-site                | top-level forge          | `portal:../forge`              |
| end2end-tests/daemon          | sdk/wardnet-js           | `portal:../../sdk/wardnet-js`  |
| (future) admin-app/mobile     | admin-app/forge-native   | `workspace:^`                  |
| (future) admin-app/mobile     | top-level forge          | `portal:../../forge`           |

`workspace:^` is reserved for siblings inside the same yarn workspace
(only admin-app has one). Cross-context links — admin-app needing the SDK,
marketing-site needing forge — use `portal:`. When the SDK eventually
publishes to npm, those `portal:` references flip to a normal version
range; nothing else changes.

### Rules

1. **`@wardnet/forge` is platform-neutral and shared.** No `react`, no
   `radix-ui`, no `react-native`. It exports `./styles.css` (the web
   manifestation of its tokens), `./tokens` (TS values), eventually
   `./types` and `./voice`. Both admin-app and marketing-site consume it.
2. **`@wardnet/forge-web` lives inside `source/admin-app/`.** It is the
   web implementation of forge primitives — Radix + Forge classes, CVA
   for variants forge doesn't define. It is never consumed by
   marketing-site (verified — site uses CSS only).
3. **`@wardnet/forge-native` will live inside `source/admin-app/`.** Same
   sibling rules as forge-web.
4. **Tokens are the shared substrate.** `source/forge/src/tokens.ts` is
   the canonical TS object. `source/forge/styles.css` is the CSS-var
   manifestation of those values for web consumers (today maintained by
   hand and matched to `tokens.ts` in lockstep — generator if drift
   becomes a problem). `forge-native` reads `tokens.ts` directly into
   `StyleSheet`.
5. **Component-API contracts are shared, implementations are not.** A
   `ButtonProps` type in `@wardnet/forge/types` will be consumed by
   `forge-web` and `forge-native` so app-side code is platform-agnostic.
6. **Compositions follow the same split.** Multi-part primitives like
   `Card.Header` / `Card.Body` / `Card.Footer` belong inside the platform
   package (forge-web today, forge-native later). Domain-coupled
   compositions (e.g., `DashboardStatCard`) stay in
   `admin-app/web/src/components/compound/`.
7. **Subpath exports are mandatory** in forge, forge-web, and forge-native.
   No barrel imports — `import { Button } from "@wardnet/forge-web/button"`,
   `@import "@wardnet/forge/styles.css"`. The marketing-site bundle must
   stay Radix-free.
8. **`source/forge/docs/` stays read-only.** Mocks (`design-system.html`,
   `screens.jsx`, `detail-screens.jsx`, `data.jsx`) are reference snapshots.
   Long-term direction: render mocks against the real package builds.
   Until then, accept drift — the packages are canonical.

### What changed in this slice

**Directory moves:**
- `source/web-ui/` → `source/admin-app/web/`
- `source/forge/` (the React primitives bootstrapped earlier) →
  `source/admin-app/forge-web/`
- `source/site/` → `source/marketing-site/`
- `design-system/` (repo root) → `source/forge/docs/`
- `styles.css` lifted from `admin-app/forge-web/src/` to top-level
  `source/forge/styles.css` so marketing-site can consume it without
  reaching into admin-app internals.

**Package shape:**
- New top-level `source/forge/` with `package.json`, `tsconfig.json`,
  `src/tokens.ts` (initial extraction — accent / status / radius / density /
  font), exports map for `./styles.css` and `./tokens`.
- New `source/admin-app/package.json` declaring `workspaces:
  ["forge-web", "web"]`. Single yarn.lock at admin-app root.
- `admin-app/web/package.json` renamed to `@wardnet/admin-web` (was
  `wardnet-ui`).
- `admin-app/forge-web/package.json` renamed to `@wardnet/forge-web` (was
  `@wardnet/forge`); dropped its `./styles.css` export (lives at top-level
  forge now); kept `./button` export.
- All 44 button imports in admin-app/web flipped from `@wardnet/forge/button`
  to `@wardnet/forge-web/button`. CSS `@import "@wardnet/forge/styles.css"`
  unchanged — it now resolves to the top-level forge package.

**Tooling:**
- `Makefile`: `WEBUI_DIR := source/admin-app/web`,
  `SITE_DIR := source/marketing-site`, new `ADMIN_DIR := source/admin-app`,
  new `FORGE_DIR := source/forge`. `init` installs in
  sdk/forge/admin-app/marketing-site. `build-web` and `check-web` do
  install at admin-app root, then run scripts in admin-app/web.
- CI workflows: `cache-dependency-path: source/admin-app/yarn.lock` for
  admin-app jobs, `source/marketing-site/yarn.lock` for site jobs. Daemon
  Dockerfiles, dockerignores, dependabot, codeql, detect-changes filters
  all updated.
- `.gitignore`: paths shifted to the new dirs.
- Daemon Rust: `wardnetd-api/src/web.rs` rust-embed `folder` and
  `wardnetd-api/src/openapi.rs` `include_bytes!` paths updated.

**`preserveSymlinks: true`** stays on in `admin-app/web/tsconfig.app.json`
and `admin-app/web/vite.config.ts`. Reason: forge-web's source still
imports React across a portal symlink boundary, and (separately)
admin-app/web's source imports forge-web across a workspace symlink.
Without the flag, real-path resolution lands in `source/admin-app/forge-web/`
which has no React installed locally — same root cause as the bootstrap
slice. Hoisting via the admin-app workspace covers types within
admin-app, but forge-web also pulls react via portal-from-top-level-forge
in a way that doesn't fully dedupe. Cleanest is to leave the flag on.

### Migration ordering (slices going forward)

1. **(Done — this slice)** Restructure: directory moves, admin-app
   workspace, top-level forge with `tokens.ts` + `styles.css`.
2. **Subsequent primitive slices** (Card, Pill, Dialog, …) land in
   `source/admin-app/forge-web/src/primitives/`. New tokens go into
   `source/forge/src/tokens.ts`; new shared types go into
   `source/forge/src/types.ts` (create the file lazily).
3. **Tokens.ts completion.** The initial `tokens.ts` covers a representative
   subset (brand, status, radius, density, font). Extracting the full
   surface / ink / sidebar / shadow / soft-variant set is a follow-up
   slice — not blocking, since `styles.css` is still authoritative for
   web rendering.
4. **`forge-native` slice** lands when the mobile app spins up — adds
   `source/admin-app/forge-native/` and `source/admin-app/mobile/`
   alongside web + forge-web. Two new workspace members; no structural
   rewrite.

### Marketing-site bundle weight

The marketing site still imports only `@wardnet/forge/styles.css` (top-level
forge — pure CSS, no Radix). When tokens are needed in JS (e.g., chart
colors), it'll import from `@wardnet/forge/tokens` — also pure TS, no
runtime overhead. The site bundle stays Radix-free by construction:
forge-web is not a dependency of marketing-site at all.

---

## Working agreements

- One PR. Branch `chore/design-system`. Sub-tasks land as commits on this branch.
- **Per session**: pick a slice (a primitive / a compound / a page / a public-site
  surface). If the slice needs anything new in Forge, change Forge first (in the
  same commit). Mark `[ ]` → `[x]` in this doc in the same commit, push.
- Commit prefix: `chore(design-system): <scope>` — e.g. `chore(design-system):
  port StatTile primitive`, `chore(design-system): rewrite Dashboard page`.
- Pre-push: `make ui-checks` (or whatever the project make targets are — verify
  on first commit) + visual sanity check in the dev server before marking done.
- Don't refactor unrelated logic. Visual layer only. If a logic bug shows up,
  file an issue, don't bundle it in.
- New components go where they fit (`core/ui/`, `compound/`, `features/`) — no
  new top-level dirs.
- Mono everywhere facts live. `<span className="font-mono">` (Tailwind utility)
  or `<span className="mono">` (Forge class) — both backed by JetBrains Mono.
- **Forge first**: never invent a token, class, or pattern in app code.
  Add it to `design-system/styles.css` (and the relevant mock) first, then
  consume it.

---

## Status legend

- `[ ]` — TODO
- `[~]` — In progress this session
- `[x]` — Done (visually verified in dev server)
- `[!]` — Blocked — see Findings note
- `[-]` — Removed from scope — see Findings note

---

## Session log

The per-slice history (one row per migration slice with its decisions, generalisations, and verification notes) lives in [DESIGN-SYSTEM-LOG.md](DESIGN-SYSTEM-LOG.md). When closing a slice, append a row there — not here.
