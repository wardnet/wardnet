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
   into `source/web-ui/src/index.css` and `source/site/src/index.css`. Both
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
5. **Public site = Ward Navy chrome.** `source/site/` retires its
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
| `tw-animate-css`       | **Keep.** Used for dialog/sheet/dropdown entrance animations. |
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
|                                   | `tw-animate-css` (entrance animations)            |                                           |
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

## Primitives — `source/web-ui/src/components/core/ui/`

Replace each shadcn wrapper with a Radix-backed Forge component. `n/a` = no
Radix primitive needed (pure visual or HTML element).

| File              | Radix              | Forge surface                                          | Status |
| ----------------- | ------------------ | ------------------------------------------------------ | ------ |
| button.tsx        | n/a                | `.btn` / `.btn--primary` / `.btn--ghost` / `.btn--danger` / `.btn--sm` | [x] |
| card.tsx          | n/a                | `.card` / `.card--flush` + `.card__head`               | [ ]    |
| badge.tsx         | n/a                | `.pill` / `.pill--ok|warn|down|info|ghost`             | [ ]    |
| dialog.tsx        | Dialog             | `.modal` (`.scrim` / `.modal__head` / `.modal__body` / `.modal__foot`) | [ ] |
| alert-dialog.tsx  | AlertDialog        | `.modal` + danger primary action                       | [ ]    |
| sheet.tsx         | Dialog (slide)     | mobile bottom sheet (Forge mobile.html §sheet)         | [ ]    |
| dropdown-menu.tsx | DropdownMenu       | popover-style menu (Forge §07)                         | [ ]    |
| popover.tsx       | Popover            | `--bg-card` + `--shadow-pop`                           | [ ]    |
| select.tsx        | Select             | field with chevron, popover list                       | [ ]    |
| switch.tsx        | Switch             | `.toggle`                                              | [ ]    |
| tabs.tsx          | Tabs               | `.tabs` (pill) for view switches; underline tabs (`.tabs-bar` from §05) for page nav | [ ] |
| radio-group.tsx   | RadioGroup         | Forge §09 form-row pattern                             | [ ]    |
| label.tsx         | Label              | `.field label` / `.read-label`                         | [ ]    |
| input.tsx         | n/a                | `.field input`                                         | [ ]    |
| textarea.tsx      | n/a                | `.field textarea`                                      | [ ]    |
| input-group.tsx   | n/a                | `.field` + helper                                      | [ ]    |
| ipv4-input.tsx    | n/a                | `.field input.mono`                                    | [ ]    |
| mac-input.tsx     | n/a                | `.field input.mono`                                    | [ ]    |
| command.tsx       | (cmdk)             | command palette popover                                | [ ]    |
| chart.tsx         | (recharts)         | Forge §10 chart rules — 4 hairline gridlines, mono Y-axis labels, no vertical grid, area + line, tooltip in `--bg-card` + `--shadow-pop` | [ ] |
| data-table.tsx    | (tanstack/table)   | `.tbl` + `.host` row pattern                           | [ ]    |
| toaster.tsx       | (sonner)           | `.toast`                                               | [ ]    |

### New primitives to introduce (don't exist yet)

| Primitive   | Source                              | Status |
| ----------- | ----------------------------------- | ------ |
| StatTile    | `design-system/primitives.jsx`      | [ ]    |
| Sparkline   | `design-system/primitives.jsx`      | [ ]    |
| Donut       | `design-system/primitives.jsx`      | [ ]    |
| Icon set    | `design-system/primitives.jsx` (Forge ships hand-tuned 1.7-stroke icons) — see Open Questions | [ ] |

---

## Compound components — `source/web-ui/src/components/compound/`

Each is a Forge-native rewrite. Move utility-class soup → Forge classes; mono-wrap
all facts; status pills via `.pill--*`.

| Component                       | Status | Notes                                                      |
| ------------------------------- | ------ | ---------------------------------------------------------- |
| AllowlistTable                  | [ ]    | `.tbl`                                                     |
| ApiErrorAlert                   | [ ]    | `.pill--down` style or full-card error                     |
| BlocklistTable                  | [ ]    | `.tbl`                                                     |
| ConfirmDialog                   | [ ]    | AlertDialog-backed                                         |
| ConnectionBanner                | [ ]    | thin top banner, mono ws status                            |
| ConnectionStatus                | [ ]    | `.pill--ok|warn|down`                                      |
| CountryCombobox                 | [ ]    | cmdk-backed, country flag prefix                           |
| CronSchedulePicker              | [ ]    | field cluster                                              |
| DashboardStatCard               | [ ]    | -> StatTile primitive                                      |
| DashboardUsageBar               | [ ]    | `.bar`                                                     |
| DetailPageHeader                | [ ]    | H1 + status pill + breadcrumb (per skill §detail page)     |
| DeviceIcon                      | [ ]    | use Forge Icon set or stay on lucide (open Q)              |
| DeviceSelect                    | [ ]    | Radix Select wrapper                                       |
| DeviceTable                     | [ ]    | `.tbl` + `.host` row                                       |
| DhcpConfigCard                  | [ ]    | edit-mode card protocol                                    |
| DhcpLeaseTable                  | [ ]    | `.tbl`                                                     |
| DhcpReservationTable            | [ ]    | `.tbl`                                                     |
| DhcpStatusCard                  | [ ]    | first-card pattern (status pill + headline number)         |
| DhcpSummaryCard                 | [ ]    | StatTile-derived                                           |
| DiscoveryPlaceholder            | [ ]    | `.empty`                                                   |
| EmptyStatePlaceholder           | [ ]    | `.empty`                                                   |
| FilterRuleTable                 | [ ]    | `.tbl`                                                     |
| HostCell                        | [ ]    | `.host` markup                                             |
| JobProgressDescription          | [ ]    | mono job state                                             |
| Logo                            | [ ]    | shield + signal mark, 26px chrome / 60px+ marketing        |
| LogViewer                       | [ ]    | `.logs` / `.logrow` + level filter (`is-warn`/`is-err`/`is-info`) |
| MobileMenu                      | [ ]    | sheet pattern                                              |
| PageHeader                      | [ ]    | `.h-title` + `.h-sub` + right-aligned actions              |
| ProfileToggleList               | [ ]    | toggle-row list (added on main 2026-05-09 in #343)         |
| RecentErrorsCard                | [ ]    | log-style list                                             |
| RoutingSelector                 | [ ]    | radio-group field                                          |
| Sidebar                         | [ ]    | `.side` + floating variant + nav items + brand mark + foot |
| StatusBadge                     | [ ]    | `.pill--*`                                                 |
| TunnelCard                      | [ ]    | `.tcard` (flag + title + grid + throughput strip)          |
| TunnelGrid                      | [ ]    | grid wrapper                                               |
| UncleanShutdownBanner           | [ ]    | banner, danger-soft tones                                  |
| UpdateBanner                    | [ ]    | banner, info-soft tones                                    |

---

## Feature components — `source/web-ui/src/components/features/`

| Component                | Status | Notes                                            |
| ------------------------ | ------ | ------------------------------------------------ |
| BackupCard               | [ ]    | edit-mode card                                   |
| CreateReservationSheet   | [ ]    | sheet (mobile) / dialog (desktop)                |
| CreateTunnelInline       | [ ]    | inline form + WireGuard config paste             |
| DashboardLogWidget       | [ ]    | `.logs`                                          |
| DeviceDnsFilterCard      | [ ]    | edit-mode card                                   |
| DeviceIdentityCard       | [ ]    | always read-only (per skill §detail)             |
| DeviceNetworkCard        | [ ]    | edit-mode card                                   |
| DeviceSettingsCard       | [ ]    | edit-mode card                                   |
| DnsFilterSettingsCard    | [ ]    | edit-mode card with toggles                      |
| DnsStatsSection          | [ ]    | qchart + donut + stat tiles                      |
| EditDhcpConfigSheet      | [ ]    | sheet                                            |
| ManualTunnelTab          | [ ]    | tabbed sub-form                                  |
| PowerCard                | [ ]    | danger-toned actions outside card per skill §detail |
| ProviderTunnelTab        | [ ]    | tabbed sub-form                                  |
| RestartProgressDialog    | [ ]    | dialog                                           |
| ShutdownProgressDialog   | [ ]    | dialog                                           |
| TunnelDevicesTable       | [ ]    | `.tbl` + `.host`                                 |
| TunnelThroughputChart    | [ ]    | line chart §10 (Download = `--accent`, Upload = `--info`) |
| UpdateCard               | [ ]    | card                                             |

---

## Layouts — `source/web-ui/src/components/layouts/`

| File         | Status | Notes                                                              |
| ------------ | ------ | ------------------------------------------------------------------ |
| AppLayout    | [ ]    | floating sidebar default; topbar with breadcrumbs + search + ⌘K kbd |
| AuthLayout   | [ ]    | centered card on `--bg` (no chrome)                                |

---

## Pages — `source/web-ui/src/pages/`

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

## Public site — `source/site/`

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
- **Public site palette**: `source/site/src/index.css` carries its own
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
5. **`design-system/styles.css` is the canonical CSS.** When tokens change
   there, both `source/web-ui/src/index.css` and `source/site/src/index.css`
   pick up the change — either by re-vendoring (copy) or by importing the
   file directly (preferred — see Findings).

### Forge update checklist (running list)

| Item                                                                          | Status |
| ----------------------------------------------------------------------------- | ------ |
| Rename `.agents/skills/using-desing-system/` → `using-design-system/`         | [x]    |
| Strip `[data-density="compact"]` block from `styles.css` (we ship comfortable only) | [x] |
| Strip non-floating sidebar variants from `styles.css` + collapse `.app[data-rail="floating"]` into `.app` | [x] |
| Decide on import strategy: vendor `styles.css` into each app, or import from `design-system/` via Vite alias (preferred — single source) | [x] |
| Document Radix-binding patterns in `design-system.html` §05 (e.g. how Switch ties to `.toggle`, how Dialog ties to `.modal`) | [ ] |
| Add Tailwind 4 `@theme inline` reference snippet in `README.md` so future apps know how to consume Forge tokens | [x] |
| Drop `tailwind.config.js` once Tailwind 4 reference is in README, OR keep as a Tailwind-3-compat reference (decide on first use) | [ ] |
| Add any new primitives we introduce in the apps back into `primitives.jsx` (StatTile already there; new ones go here too) | [ ] |
| Update `design-system.html` §05 Components when we add new components in code | [ ]    |
| Bootstrap `source/forge/` workspace package (first primitive slice — see "Where Forge lives" below) | [x] |
| Move `styles.css` from `design-system/` to `source/forge/` once package exists; retarget `@wardnet/forge` alias | [x] |
| Convert `design-system/` to docs-only (mocks + studio HTML) — primitives.jsx mocks may be retired or rendered against `source/forge/` | [ ] |
| Delete legacy shadcn-token alias block in `source/web-ui/src/index.css` once no component references the old utilities (`bg-background`, `text-foreground`, `bg-primary`, `border-border`, `border-input`, `ring-ring`, `bg-sidebar*`, `bg-destructive`, `bg-muted`, `bg-success`, `bg-warning`, `bg-popover`, `bg-secondary`, `text-muted-foreground`, etc.) | [ ] |
| Delete legacy `--brand-indigo` / `--brand-slate` / `--brand-green` / `--brand-green-hover` aliases in `source/site/src/index.css` once site components consume Forge tokens (`var(--accent)`, `bg-accent`, etc.) | [ ] |

---

## Where Forge lives (locked 2026-05-09)

End-state layout:

```
source/forge/          ← runtime workspace package (@wardnet/forge)
  package.json
  src/
    primitives/        ← Button, Card, Pill, Dialog, Switch, … (Radix + Forge classes)
    styles.css         ← THE canonical CSS (tokens + component classes)
  exports map          ← subpath exports per primitive for tree-shaking

design-system/         ← docs-only "studio" (visual reference + mocks)
  README.md
  design-system.html
  screens.jsx          ← legacy babel mocks; eventually rendered against source/forge/
  detail-screens.jsx
```

### Why a workspace package and not just a folder

- **Single home for runtime artifacts.** Primitives, the CSS, and the Radix dep all live together. Today web-ui owns Radix and re-implements primitives the site can never reuse; tomorrow they're shared.
- **Matches existing convention.** `source/sdk/wardnet-js` is already a workspace package consumed via portal (`@wardnet/js`). `source/forge` follows the same pattern under `@wardnet/forge`.
- **Stable import paths.** `@wardnet/forge/styles.css` and `@wardnet/forge/button` work whether Forge is a portal, a workspace package, or eventually a published npm package — only the resolution mechanism changes, not the consumer code.

### Rules

1. **Primitives and Radix live in `source/forge/`.** Apps consume primitives — they don't import Radix directly. Radix becomes a transitive dep of `@wardnet/forge`.
2. **Forge classes are applied directly inside the primitive.** Forge's CSS is the styling source; CVA is reserved for variants Forge doesn't define (e.g., size variants beyond Forge's `--sm`). The primitive's job is to wrap Radix behavior in Forge classes — not to reimplement styling via utilities.
3. **Subpath exports are mandatory.** `import { Button } from "@wardnet/forge"` (barrel) pulls every primitive and every Radix submodule into the consuming bundle. Use `import { Button } from "@wardnet/forge/button"`. The site (which only uses tokens/CSS today, no primitives) must be able to consume Forge without dragging Radix into its bundle.
4. **`design-system/` is read-only.** Once primitives move to the package, the old `primitives.jsx` Babel mocks become reference-only. Rendering mocks against `source/forge/` builds is the long-term direction; until then, mocks may drift from primitives — accept it, the package is canonical.

### Migration mechanics

- **This (foundation) slice doesn't move anything.** The Vite alias `@wardnet/forge → design-system/` is forward-compatible: when `source/forge/` exists, only the alias target moves; no consumer code changes.
- **First primitive slice bootstraps `source/forge/`.** Picks one small primitive (Button is the candidate), creates the package, ports the primitive in, retargets the alias, deletes `source/web-ui/src/components/core/ui/button.tsx`. Subsequent primitive slices are smaller because the package already exists.
- **`styles.css` moves to `source/forge/`** as part of the bootstrap slice. The alias target moves with it; consumer `@import "@wardnet/forge/styles.css"` lines don't change.
- **`design-system/` is converted last.** Once nothing in the apps references `design-system/*` directly, we can move primitives.jsx mocks to be docs-only or render against the real package.

### Site bundle weight

The public site doesn't use Radix today (verified). It will continue to consume only `@wardnet/forge/styles.css` (tokens + CSS). Subpath exports keep this clean — the site bundle never pulls in Radix unless we explicitly import a primitive into the site.

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

---

## Session log

| Date       | Session focus                                     | Commits         |
| ---------- | ------------------------------------------------- | --------------- |
| 2026-05-08 | Strategy locked, all 8 open questions answered, Forge promoted to source-of-truth, doc bootstrapped, audit complete | (this commit) |
| 2026-05-09 | Foundation slice: Forge updates (skill rename, density block stripped, floating rail collapsed into `.app`, README import-strategy doc); web-ui font swap to Inter Tight + JetBrains Mono, `index.css` rewritten on Forge tokens via `@theme inline`, `useTheme` hook switched to `data-theme` attribute, legacy shadcn-token aliases retained for build compatibility; site font swap, `index.css` rewritten on Forge tokens, `--brand-*` vars retained as Forge-mapped aliases; Vite alias `@wardnet/forge` added in both apps targeting `design-system/`; "Where Forge lives" strategy locked (`source/forge/` workspace package — bootstrap in first primitive slice). Type-check + build pass for both apps; site format clean. | (this commit) |
| 2026-05-09 | Forge bootstrap + Button primitive: created `source/forge/` workspace package (`@wardnet/forge` portal:, subpath exports for `./styles.css` + `./button`); `styles.css` moved from `design-system/` to `source/forge/src/`; `radix-ui` moved from web-ui deps to forge deps (apps consume primitives, not Radix). Vite alias `@wardnet/forge` dropped in both apps — exports-map resolution via portal protocol replaces it. `preserveSymlinks: true` set in web-ui's `tsconfig.app.json` and `vite.config.ts` so cross-package source imports reach web-ui's `react` / `@types/react`. Button primitive ported to `source/forge/src/primitives/button.tsx` using Radix `Slot.Root` for `asChild` and Forge `.btn` / `.btn--primary` / `.btn--ghost` / `.btn--danger` / `.btn--sm` / `.btn--icon` classes; legacy shadcn variant strings (`outline`/`secondary`/`destructive`/`tertiary` and sizes `sm`/`icon`/`icon-sm`) kept as the public API and mapped to Forge classes via CVA so call sites stay stable. `source/web-ui/src/components/core/ui/button.tsx` deleted; 44 imports retargeted to `@wardnet/forge/button`. Type-check + lint clean for web-ui (1 pre-existing prettier error in `Step4RouterMac.tsx` unchanged); type-check + format:check clean for site. Build verified for both apps. | (this commit) |
