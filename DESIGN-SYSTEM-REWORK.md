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

## Primitives — `source/admin-app/web/src/components/core/ui/`

Replace each shadcn wrapper with a Radix-backed Forge component. `n/a` = no
Radix primitive needed (pure visual or HTML element).

| File              | Radix              | Forge surface                                          | Status |
| ----------------- | ------------------ | ------------------------------------------------------ | ------ |
| button.tsx        | n/a                | `.btn` / `.btn--primary` / `.btn--ghost` / `.btn--danger` / `.btn--sm` | [x] |
| card.tsx          | n/a                | `.card` / `.card--flush` + `.card__head` + `.card__foot` (added) | [x] |
| badge.tsx → pill  | n/a                | `.pill` / `.pill--ok|warn|down|info|ghost` (renamed `Badge` → `Pill`) | [x] |
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

## Compound components — `source/admin-app/web/src/components/compound/`

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

## Feature components — `source/admin-app/web/src/components/features/`

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
| Document Radix-binding patterns in `design-system.html` §05 (e.g. how Switch ties to `.toggle`, how Dialog ties to `.modal`) | [ ] |
| Add Tailwind 4 `@theme inline` reference snippet in `README.md` so future apps know how to consume Forge tokens | [x] |
| Drop `tailwind.config.js` once Tailwind 4 reference is in README, OR keep as a Tailwind-3-compat reference (decide on first use) | [ ] |
| Add any new primitives we introduce in the apps back into `primitives.jsx` (StatTile already there; new ones go here too) | [ ] |
| Update `design-system.html` §05 Components when we add new components in code | [ ]    |
| Bootstrap `source/forge/` workspace package (first primitive slice — see "Where Forge lives" below) | [x] |
| Move `styles.css` from `design-system/` to `source/forge/` once package exists; retarget `@wardnet/forge` alias | [x] |
| **Restructure: context-per-source-dir + admin-app internal workspace.** Rename `source/web-ui/` → `source/admin-app/web/`; move `source/forge/` (Radix primitives) → `source/admin-app/forge-web/`; rename `source/site/` → `source/marketing-site/`; move repo-root `design-system/` → `source/forge/docs/`. Create new top-level `source/forge/` (platform-neutral) with `tokens.ts` + `styles.css` + exports map. Create `source/admin-app/package.json` (workspaces: web, forge-web). Flip 44 button imports to `@wardnet/forge-web/button`. Update Makefile, CI, gitignore, daemon rust-embed paths. (See "Where Forge lives — context-per-source-dir + admin-app workspace" below.) | [x] |
| **Reserve `source/admin-app/forge-native/` and `source/admin-app/mobile/`** for the future React Native primitives + mobile bundle. No code in this branch — placeholder rule that the names are taken. | [ ] |
| Complete `tokens.ts` extraction — initial slice covered brand / status / radius / density / font; surfaces (`--bg`, `--bg-elev`, …), ink (`--ink`, `--ink-2`, …), sidebar (`--side-*`), shadows, and soft-variant pairs still need transcribing. Until then `styles.css` is authoritative for web rendering. | [ ] |
| Convert `source/forge/docs/` to docs-only (mocks + studio HTML rendered against the real package builds long-term). | [ ] |
| Delete legacy shadcn-token alias block in `source/admin-app/web/src/index.css` once no component references the old utilities (`bg-background`, `text-foreground`, `bg-primary`, `border-border`, `border-input`, `ring-ring`, `bg-sidebar*`, `bg-destructive`, `bg-muted`, `bg-success`, `bg-warning`, `bg-popover`, `bg-secondary`, `text-muted-foreground`, etc.) | [ ] |
| Delete legacy `--brand-indigo` / `--brand-slate` / `--brand-green` / `--brand-green-hover` aliases in `source/marketing-site/src/index.css` once site components consume Forge tokens (`var(--accent)`, `bg-accent`, etc.) | [ ] |

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

---

## Session log

| Date       | Session focus                                     | Commits         |
| ---------- | ------------------------------------------------- | --------------- |
| 2026-05-08 | Strategy locked, all 8 open questions answered, Forge promoted to source-of-truth, doc bootstrapped, audit complete | (this commit) |
| 2026-05-09 | Foundation slice: Forge updates (skill rename, density block stripped, floating rail collapsed into `.app`, README import-strategy doc); web-ui font swap to Inter Tight + JetBrains Mono, `index.css` rewritten on Forge tokens via `@theme inline`, `useTheme` hook switched to `data-theme` attribute, legacy shadcn-token aliases retained for build compatibility; site font swap, `index.css` rewritten on Forge tokens, `--brand-*` vars retained as Forge-mapped aliases; Vite alias `@wardnet/forge` added in both apps targeting `design-system/`; "Where Forge lives" strategy locked (`source/forge/` workspace package — bootstrap in first primitive slice). Type-check + build pass for both apps; site format clean. | (this commit) |
| 2026-05-09 | Forge bootstrap + Button primitive: created `source/forge/` workspace package (`@wardnet/forge` portal:, subpath exports for `./styles.css` + `./button`); `styles.css` moved from `design-system/` to `source/forge/src/`; `radix-ui` moved from web-ui deps to forge deps (apps consume primitives, not Radix). Vite alias `@wardnet/forge` dropped in both apps — exports-map resolution via portal protocol replaces it. `preserveSymlinks: true` set in web-ui's `tsconfig.app.json` and `vite.config.ts` so cross-package source imports reach web-ui's `react` / `@types/react`. Button primitive ported to `source/forge/src/primitives/button.tsx` using Radix `Slot.Root` for `asChild` and Forge `.btn` / `.btn--primary` / `.btn--ghost` / `.btn--danger` / `.btn--sm` / `.btn--icon` classes; legacy shadcn variant strings (`outline`/`secondary`/`destructive`/`tertiary` and sizes `sm`/`icon`/`icon-sm`) kept as the public API and mapped to Forge classes via CVA so call sites stay stable. `source/admin-app/web/src/components/core/ui/button.tsx` deleted; 44 imports retargeted to `@wardnet/forge/button`. Type-check + lint clean for web-ui (1 pre-existing prettier error in `Step4RouterMac.tsx` unchanged); type-check + format:check clean for site. Build verified for both apps. | (this commit) |
| 2026-05-09 | **Architecture revision — platform split locked + yarn workspaces locked.** Decision: Forge splits into three packages — `@wardnet/forge` (platform-neutral: tokens, types, voice), `@wardnet/forge-web` (Radix + CSS classes — the package that exists today, just mis-named), `@wardnet/forge-native` (future RN). Compositions (Card.Header etc.) live alongside primitives in the platform package, not a separate "forge-ui." Domain-coupled compositions stay in `web-ui/components/compound/`. Yarn workspaces replace `portal:` once we have ≥3 packages — root `package.json` with `workspaces` array, single `yarn.lock`, `workspace:^` for intra-repo deps. Doc-only commit: rewrote "Where Forge lives" with the platform-split architecture, the workspaces rationale, end-state layouts, full rules set, and a detailed impacts list (code/layout, tooling, runtime, risk surface, migration ordering). Added two new tasks to the Forge update checklist: (1) yarn workspaces conversion slice; (2) `forge` ⇄ `forge-web` rename slice. No code change in this commit. | (this commit) |
| 2026-05-09 | **Architecture revision again — root workspace abandoned in favour of admin-app-internal workspace + context-per-source-dir.** During the workspace conversion the bigger structural concern surfaced: `source/<thing>/` is already organised by deployment unit (daemon / SDK / site / admin / e2e). Hoisting forge / forge-web / forge-native to the same level would have flattened that segregation. New decision: yarn workspace lives **inside `source/admin-app/`** (containing `web` + `forge-web`, and later `mobile` + `forge-native`). Top-level `source/forge/` is the platform-neutral design language, consumed by both admin-app/web AND marketing-site. SDK stays top-level (separate cadence, will be published). Big restructure landed in this slice: `source/web-ui/` → `source/admin-app/web/`; old `source/forge/` (React primitives) → `source/admin-app/forge-web/`; `source/site/` → `source/marketing-site/`; repo-root `design-system/` → `source/forge/docs/`. New top-level `source/forge/` with `tokens.ts` (initial extraction — brand, status, radius, density, font) + `styles.css` (lifted out of forge-web). All 44 button imports retargeted to `@wardnet/forge-web/button`. Makefile, CI workflows, gitignore, daemon rust-embed paths, dependabot, codeql, detect-changes filters all updated. Type-check + lint + build clean for admin-app/web; type-check + format:check + build clean for marketing-site (1 pre-existing prettier error in `Step4RouterMac.tsx` unchanged). | (this commit) |
| 2026-05-09 | Card primitive port (second primitive — multi-part). Mapped legacy 7-component API (`Card`/`CardHeader`/`CardTitle`/`CardDescription`/`CardAction`/`CardContent`/`CardFooter`) onto Forge's `.card` / `.card__head` (with auto-styled nested `h3`, `.sub`, `.right`) and a new `.card__foot` class added to `source/forge/styles.css` per the Forge-first rule. Export shape locked as flat named exports — kept the 29 call sites' import shape stable so the migration was a pure import-path retarget. Caught a flush prop in review and removed it: replaced the consumer-facing `flush` prop with a CSS `:has()` rule (`.card:has(> .card__head), .card:has(> .card__foot) { padding: 0; }`) so layout follows from composition rather than from a prop the consumer might mis-set. `.card--flush` stays in Forge for explicit no-head/no-foot cases (image-only, table-only) and CSS-only consumers. Legacy `core/ui/card.tsx` deleted; 29 imports retargeted to `@wardnet/forge-web/card`. Type-check + lint + build clean for admin-app/web (1 pre-existing prettier error in `Step4RouterMac.tsx` unchanged); type-check + format:check clean for marketing-site. | (this commit) |
| 2026-05-09 | Pill primitive port (third primitive — first variant rename). Renamed component `Badge` → `Pill` to match Forge's class vocabulary; renamed legacy variant strings to Forge's semantic vocabulary (`success` → `ok`, `destructive` → `down`, `outline`/`secondary` → `ghost`; added `warn`/`info` where call sites had been forced into stylistic substitutes). Migration was wider than Button/Card (touched the `StatusBadge` wrapper's `variantForTone` map, `LogViewer.levelVariant`, `DnsLogs.RESULT_BADGE` lookup table, and 9 call sites' import + JSX) but tractable at this size — captured in Findings as the rule "adopt Forge variant vocabulary when semantic, keep legacy when stylistic, weight by call-site count." Forge `.pill` / `.pill--*` classes used directly via CVA; primitive supports `asChild` via Radix `Slot.Root` (matches Button). New `./pill` subpath export in forge-web. Legacy `core/ui/badge.tsx` deleted. Type-check + lint + build clean for admin-app/web (1 pre-existing prettier error in `Step4RouterMac.tsx` unchanged); type-check + format:check clean for marketing-site. | (this commit) |
