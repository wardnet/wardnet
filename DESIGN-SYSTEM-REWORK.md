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
| `tw-animate-css`       | **Keep (re-evaluate).** Originally retained for dialog/sheet/dropdown entrance animations. With Sheet dropped (see Findings) and Modal/AlertModal motion now living in Forge keyframes, the remaining justification is variant-specific motion in DropdownMenu / Popover / Select. The Popover slice will tell us whether those land Forge keyframes (consistent with Modal) or tw-animate-css utilities. |
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
| dialog.tsx → modal | Dialog            | `.modal` (`.scrim` / `.modal__head` / `.modal__body` / `.modal__foot`) (renamed `Dialog` → `Modal`) | [x] |
| alert-dialog.tsx → alert-modal | AlertDialog | `.modal` (no new modifier — danger framing is button-level; renamed `AlertDialog` → `AlertModal`) | [x] |
| sheet.tsx         | —                  | dropped — replaced by inline detail/edit pattern (see Findings) | [-]    |
| dropdown-menu.tsx | DropdownMenu       | `.popover` surface + `.menu-item` / `.menu-separator` (added) | [x] |
| popover.tsx       | Popover            | `.popover` (added — `--bg-card` + `--shadow-pop` + side-aware entrance) | [x] |
| select.tsx        | Select             | `.select-trigger` (added) + `.popover` `.select-content` (added) + `.menu-item` (incl. `[data-state="checked"]::after` checkmark, added) | [x] |
| switch.tsx → toggle | Switch           | `.toggle` (renamed `Switch` → `Toggle`)                | [x]    |
| tabs.tsx          | Tabs               | `.tabs` (pill) for view switches — only surface this slice; legacy `variant="line"` had zero call sites and was dropped. Underline page-nav surface (`.tabs-bar`) deferred until a router-driven nav consumer needs it. | [x] |
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
| MobileMenu                      | [ ]    | sheet dropped — new mobile-nav approach TBD (own slice)    |
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
| CreateReservationSheet   | [ ]    | inline create form (sheet dropped — likely renamed CreateReservationInline) |
| CreateTunnelInline       | [ ]    | inline form + WireGuard config paste             |
| DashboardLogWidget       | [ ]    | `.logs`                                          |
| DeviceDnsFilterCard      | [ ]    | edit-mode card                                   |
| DeviceIdentityCard       | [ ]    | always read-only (per skill §detail)             |
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
- `[-]` — Removed from scope — see Findings note

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
| 2026-05-09 | Toggle primitive port (fourth primitive — first Radix-wrap template). Renamed `Switch` → `Toggle` to match Forge's `.toggle` class; the legacy `core/ui/switch.tsx` is gone. Two rules locked in this slice: (1) **component name always follows Forge class name — no Radix exception**, supersedes the Pill slice's "modulo Radix" wording; the Radix umbrella import keeps its Radix name inside the file (`import { Switch } from "radix-ui"`, render `<Switch.Root>`), but the file/export/subpath are Forge's name. (2) **Radix `data-state` ↔ Forge modifier classes are bridged in Forge's stylesheet, not in JS** — added `.toggle[data-state="checked"]` selectors next to `.toggle.is-on` so CSS-only consumers and React primitives land on the same visual. This is the template for every state-bearing Radix-wrap that follows (Tabs, RadioGroup, Dialog→Modal, Popover, Sheet, AlertDialog, Select, DropdownMenu) — Forge will grow a family of `[data-state="…"]` selectors and that's deliberate. Toggle primitive is minimal: `<Switch.Root className="toggle" {...props} />`, no `Switch.Thumb` child since Forge's `.toggle::after` provides the thumb (a generalisable rule — don't render Radix sub-components Forge already owns visually). Subpath `./toggle` added to `@wardnet/forge-web`; 9 call sites retargeted (`@/components/core/ui/switch` → `@wardnet/forge-web/toggle`, `<Switch …>` → `<Toggle …>`); one stale JSDoc reference in `ProfileToggleList.tsx` updated. Legacy alias audit: switch.tsx's referenced aliases (`bg-input`, `bg-primary`, `bg-background`, `bg-foreground`, `bg-primary-foreground`, `border-ring`, `ring-ring`, `border-destructive`, `ring-destructive`) all retain other consumers — no rows newly zero-referenced. Type-check + lint + build clean for admin-app/web (1 pre-existing prettier error in `Step4RouterMac.tsx` unchanged); type-check + format:check clean for marketing-site. | (this commit) |
| 2026-05-09 | Modal primitive port (fifth primitive — first multi-part-Radix-wrap template). Renamed `Dialog` → `Modal` to match Forge's `.modal` class; the legacy `core/ui/dialog.tsx` is gone. The Radix umbrella import keeps its Radix name inside the file: `import { Dialog } from "radix-ui"`, render `<Dialog.Root>` / `<Dialog.Portal>` / `<Dialog.Overlay>` / `<Dialog.Content>` etc. Multi-part export shape (`Modal` / `ModalTrigger` / `ModalContent` / `ModalHeader` / `ModalTitle` / `ModalDescription` / `ModalBody` / `ModalFooter` / `ModalClose`) follows the flat-named-exports precedent set by Card. Each export classified as one of four shapes — **passthrough Radix** (Modal, ModalTrigger, ModalDescription, ModalClose), **Forge-owned div** (ModalHeader = `.modal__head`, ModalBody = `.modal__body`, ModalFooter = `.modal__foot` — generalises Toggle's "don't render Radix sub-components Forge already owns visually" from a single pseudo-element to whole structural divs), **composed Radix tree** (ModalContent = Portal + Overlay[scrim] + Content[modal]), or **Radix part wearing Forge's expected element via asChild** (ModalTitle renders `<Dialog.Title asChild><h3>...</h3></Dialog.Title>` so a11y wiring lands on the `<h3>` Forge's `.modal__head h3` selector already styles). Forge gained the second instance of the data-state bridge: `.scrim[data-state="open"\|"closed"]` and `.modal[data-state="open"\|"closed"]` selectors with new `scrim-in/out` + `modal-in/out` keyframes; `.modal` was given fixed-positioning + a `.scrim > .modal { position: static; transform: none }` reset so Radix's sibling Overlay/Content tree and CSS-only mocks (modal nested inside scrim) both render correctly. Locked the rule on where animations live: **intrinsic** to `.scrim` / `.modal` → Forge keyframes scoped to `[data-state="…"]`; **variant-specific** (e.g. Sheet's slide-from-bottom) → tw-animate-css utilities on the primitive's className. Subpath `./modal` added to `@wardnet/forge-web`; 2 call sites migrated (`core/ui/command.tsx` wrapper + `features/BackupCard.tsx`'s ExportDialog & RestoreDialog) — full Dialog* → Modal* JSX rename plus added explicit `<ModalBody>` wrappers around the body content per Forge's structural expectation (Forge's `.modal__body` provides body padding; the legacy DialogContent's grid+gap pattern is gone). Note on `Dialog.Trigger`: Radix's own `asChild` prop covers the Slot wrapping that Button needed to add explicitly — ModalTrigger is a one-line passthrough. Legacy `dialog.tsx` deleted. Legacy alias audit: dialog.tsx's shadcn token aliases (`bg-background`, `bg-muted`, `text-muted-foreground`, `bg-popover`, `text-popover-foreground`) all retain other consumers — no rows newly zero-referenced; the `data-open:animate-in` / `data-closed:animate-out` / `data-open:fade-in-0` / `data-open:zoom-in-95` etc. utilities are tw-animate-css (kept) rather than shadcn-token aliases, and their replacement now lives in Forge's stylesheet. Type-check + lint + build clean for admin-app/web (1 pre-existing prettier error in `Step4RouterMac.tsx` unchanged); type-check + format:check clean for marketing-site. | (this commit) |
| 2026-05-09 | AlertModal primitive port (sixth primitive — multi-part Radix-wrap on a shared Forge surface). Renamed `AlertDialog` → `AlertModal` to anchor the React vocabulary on the `.modal` class (same surface as Modal) while the `Alert` prefix marks Radix 's behavioral semantics (role="alertdialog", forced confirmation, overlay-click suppressed). Going-in expectation was that Forge needed a `.modal--danger` / `.modal--alert` modifier — reading the legacy file dissolved that: `AlertDialogAction` defaulted to `variant="default"` and accepted a `variant` prop, so danger framing was always per-call-site on the *button*, never on the modal surface. Conclusion: Forge needs nothing new for this slice; visuals match Modal token-for-token. Action/Cancel ported as **pure Radix passthroughs** (legacy baked a `<Button>` wrapper with a `variant` prop — that 's the same escape-hatch shape the Card slice locked against; consumers now wire `<AlertModalAction asChild><Button variant="destructive">…</Button></AlertModalAction>`). New `./alert-modal` subpath export in `@wardnet/forge-web`. Four call sites migrated (`compound/ConfirmDialog`, `features/PowerCard` — three dialogs, `features/ShutdownProgressDialog`, `features/RestartProgressDialog`); JSX expanded by one wrapper per action/cancel — eight new `<Button asChild>` wrappers across the four files. Legacy `core/ui/alert-dialog.tsx` deleted; `AlertDialogMedia` (zero call sites) and `AlertDialogPortal`/`AlertDialogOverlay` (consumed via `AlertDialogContent` only) dropped from the surface. State-bridge selectors inherited unchanged from the Modal slice (`.scrim` / `.modal` `[data-state="open"\|"closed"]` already in `styles.css`). Type-check + lint + build clean for admin-app/web (1 pre-existing prettier error in `Step4RouterMac.tsx` unchanged); type-check + format:check clean for marketing-site. | (this commit) |
| 2026-05-09 | **Sheet dropped from design + Popover primitive port (seventh primitive — first floating-non-modal Radix-wrap).** Two parts in one commit. **Part 1 — Sheet drop (doc-only):** sheets fragment the navigation model — they slide a second surface on top of the page and force the user to hold two contexts to edit one record. Inline detail/edit is the same affordance with one fewer layer, and falls out of the edit-mode-card protocol that DhcpConfigCard / DeviceSettingsCard / BackupCard already use. Ripples: new `[-]` "Removed from scope" status legend row added; `sheet.tsx` primitive marked `[-]`; `EditDhcpConfigSheet` marked `[-]` (folds into DhcpConfigCard edit-mode); `CreateReservationSheet` notes updated (likely renamed CreateReservationInline — own slice); `MobileMenu` notes updated (no Sheet → mobile-nav approach TBD); `tw-animate-css` Locked-defaults note rewritten to "Keep (re-evaluate)" since Sheet's slide-from-bottom — the canonical variant-specific-motion example in the Modal-slice animation rule — no longer exists; multi-part Radix-wrap template still holds for Popover/DropdownMenu/Select. **Part 2 — Popover port:** Forge had no `.pop`/`.popover`/`.menu` class; added `.popover` to `source/forge/styles.css` per the Forge-first rule (`--bg-card` background, `--shadow-pop` shadow, `--radius` corner, 1px `--line` border, default 10px padding). Naming choice: `.popover` (not `.pop`) — the `--shadow-pop` token and `pop` keyframe (toast) keep their terse names, but component classes follow the React-vocabulary name (`Popover` ↔ `.popover`) per the Toggle-slice rule. Animations live in Forge per the Modal-slice intrinsic-vs-variant rule: side-aware entrance is intrinsic to popover-style floating surfaces (every popover slides from its anchor side), so four `[data-state="open"][data-side="…"]` open keyframes (`popover-in-bottom/top/left/right` — translate from anchor + scale 0.97→1) plus a direction-agnostic `popover-out` (fade + zoom-out, no slide) live in `styles.css`. `transform-origin` honors Radix's `--radix-popover-content-transform-origin` var with a `center` fallback for CSS-only consumers. **No tw-animate-css used here** — Sheet's removal didn't sink the rule, and Popover proves intrinsic-and-side-aware-as-Forge-keyframes works for floating surfaces; question of whether DropdownMenu / Select land utility-side motion is now open until concrete need surfaces. Surface stripped to three exports — `Popover` / `PopoverTrigger` / `PopoverContent` — since `PopoverAnchor` / `PopoverHeader` / `PopoverTitle` / `PopoverDescription` had zero call sites. Multi-part template stresses *downward* this slice (3 exports across 2 of 4 shapes) — DropdownMenu (next, ten-plus parts) will stress upward. New `./popover` subpath export in `@wardnet/forge-web`; 2 call sites migrated (`compound/CountryCombobox` + `compound/CronSchedulePicker`) — pure import-path retarget, no JSX rewrite (the legacy primitive's `align="center"` / `sideOffset=4` defaults preserved). Legacy `core/ui/popover.tsx` deleted. Legacy alias audit: counts dropped slightly (`bg-popover` 6→5, `text-popover-foreground` 4→3, `text-muted-foreground` ~230→~230) but no aliases newly zero-referenced. Type-check + lint + build clean for admin-app/web (1 pre-existing prettier error in `Step4RouterMac.tsx` unchanged); type-check + format:check clean for marketing-site. | (this commit) |
| 2026-05-09 | DropdownMenu primitive port (eighth primitive — second proof point for the side-aware-keyframe approach, and the first slice to surface that "stress-tests the multi-part template upward" was the wrong frame). Going-in scope listed ten-plus Radix parts (DropdownMenu / Trigger / Portal / Content / Item / CheckboxItem / RadioItem / RadioGroup / Label / Separator / Sub / SubTrigger / SubContent / Group / ItemIndicator / Shortcut). Reading the legacy `core/ui/dropdown-menu.tsx` collapsed the surface to **five exports** — `DropdownMenu` / `DropdownMenuTrigger` / `DropdownMenuContent` / `DropdownMenuItem` (with `variant: "default" \| "destructive"` prop) / `DropdownMenuSeparator` — and three call sites (AllowlistTable, FilterRuleTable, BlocklistTable) used exactly those five. So Decisions 3 (CheckboxItem / RadioItem indicator strategy) and 4 (Sub\*) dissolved trivially by the locked rule "drop features the migration doesn't require" (same calculus that stripped Popover from seven exports to three). **Forge-first:** added `.menu-item` and `.menu-separator` to `source/forge/styles.css` (after the popover keyframes); `DropdownMenuContent` reuses `className="popover"` for the surface — same `.popover` class as the Popover slice, no parallel `.menu` class, so the intrinsic side-aware motion (`[data-state="open"][data-side="…"]` keyframes from the Popover slice) is inherited verbatim. `.menu-item` uses `[data-highlighted]` (Radix-driven row highlight → `--bg-sunken`), `[data-disabled]` (opacity), and `[data-variant="destructive"]` (Forge `--danger` for rest, `--danger-soft` / `--danger-soft-ink` for highlighted) — destructive framing modeled as a `data-*` attribute (matches the existing `data-state` / `data-side` / `data-highlighted` family on Radix-wrap primitives) rather than a `.menu-item--destructive` modifier class, so the CSS selector vocabulary stays uniform. `.menu-separator` is `1px var(--line)` with `4px -6px` margin so it bleeds past the popover's 10px padding to span full width. **Multi-part template:** DropdownMenu lands at five exports across two shapes — passthrough Radix (`DropdownMenu`, `DropdownMenuTrigger`) and Radix-part-bearing-Forge-class (`DropdownMenuContent` = popover, `DropdownMenuItem` = menu-item, `DropdownMenuSeparator` = menu-separator). Modal remains the largest port (nine exports across all four shapes); the upward stress was driven by *consumer* usage, not Radix exposure — locked as a Findings clarification. **Animation rule (third application):** `.popover` keyframes inherited; **no tw-animate-css used**. Open question on tw-animate-css removal narrows to Select alone — if Select.Content also lands `.popover`, the dependency can be dropped in that slice. Subpath `./dropdown-menu` added to `@wardnet/forge-web`; 3 call sites retargeted (`@/components/core/ui/dropdown-menu` → `@wardnet/forge-web/dropdown-menu`) — pure import-path swap, no JSX rewrite (the legacy primitive's `align="end"` / `sideOffset=4` defaults preserved; `variant` prop on Item preserved as-is). Legacy `core/ui/dropdown-menu.tsx` deleted. Legacy alias audit: deletion dropped two rows to two consumers each — `bg-popover` (5→2) and `text-popover-foreground` (3→2), both with their remaining consumers in the primitive-port queue (`select.tsx` + `command.tsx`); next two slices touching those files set up both rows for deletion in the alias-pruning slice. Type-check + lint + build clean for admin-app/web (1 pre-existing prettier error in `Step4RouterMac.tsx` unchanged); type-check + format:check clean for marketing-site. | (this commit) |
| 2026-05-09 | Select primitive port (ninth primitive — last Popover-shaped positioning consumer in the Radix-wrap queue, which made it the slice that decides whether `tw-animate-css` can be removed). Surface stripped to **five exports** — `Select` / `SelectTrigger` / `SelectValue` / `SelectContent` / `SelectItem` — by surveying the 10 call sites (UpdateCard, DashboardLogWidget, DeviceSettingsCard, ProviderTunnelTab, DeviceSelect, RoutingSelector, CronSchedulePicker, DnsLogs, MyDevice, Step3DhcpOnboarding); zero use `Group` / `Label` / `Separator` / `ScrollUpButton` / `ScrollDownButton`, dropped per "drop features the migration doesn't require." `Viewport`, `ItemText`, `ItemIndicator`, `Portal` are internal. **Forge-first:** added `.select-trigger` (input visuals — `--bg-elev` background, `--line` border, 8px radius, accent focus ring — plus flex+space-between layout and a chevron `::after` mask-image), `.select-content` as additive modifier on `.popover` (min-width tied to `--radix-select-trigger-width`, max-height-with-scroll, transform-origin overrides `.popover`'s Popover-var default with `--radix-select-content-transform-origin`). Items reuse `.menu-item` with two new Select-only extensions: `.menu-item[data-state] { padding-right: 26px }` reserves indicator gutter, and `.menu-item[data-state="checked"]::after { … mask-image }` paints the check (only Radix Select.Item emits `data-state`; DropdownMenu.Item doesn't, so these selectors don't pollute existing menus). `SelectContent` renders `<RadixSelect.Content className="popover select-content">` so the four side-aware popover keyframes apply unchanged — third slice in a row to validate the intrinsic-motion-as-Forge-keyframes rule. Default `position` switched from legacy `"item-aligned"` → `"popper"` (so `data-side` is emitted and side-aware keyframes trigger); default `align` switched from `"center"` → `"start"`; no call site passes either prop explicitly. **Forge owns the icons:** chevron and checkmark are CSS `::after` pseudo-elements with mask-image data URIs (lucide stroke-2 path geometry), keying on `currentColor` for the check and `--ink-3` for the chevron — primitive renders zero icon JSX, matching the Toggle-slice rule "don't render Radix sub-components Forge already owns visually" (Select.Icon and Select.ItemIndicator both dropped). **Multi-part template:** five exports across two shapes — passthrough Radix (`Select`, `SelectValue`) and Radix-part-bearing-Forge-class (`SelectTrigger`, composed `SelectContent` = Portal+Content with `.popover .select-content`, composed `SelectItem` = Item+ItemText with `.menu-item`); fits between DropdownMenu (5/2) and Popover (3/2) on the same axis Modal anchors at the top (9/4). Subpath `./select` added to `@wardnet/forge-web`; 10 call sites retargeted (`@/components/core/ui/select` → `@wardnet/forge-web/select`) — pure import-path swap, no JSX rewrite (export shape preserved). Legacy `core/ui/select.tsx` deleted. **tw-animate-css removal call:** _blocked, but not by a primitive_ — `core/ui/sheet.tsx` is still in the tree pending its three consumers' migrations (`EditDhcpConfigSheet`, `CreateReservationSheet`, `MobileMenu`) and is the sole remaining file with `data-open:animate-in` / `data-closed:animate-out` / `data-[side=…]:data-open:slide-in-from-…-10` etc. So the dep stays in `package.json` + `index.css` `@import` until whichever feature slice last touches Sheet replaces it; that slice closes the open question. Legacy alias audit: deletion dropped `bg-popover` (2→1) and `text-popover-foreground` (2→1) — both now have `core/ui/command.tsx` as their sole remaining consumer; Command port (post-tabs/forms slice) zeroes them. Type-check + lint + build clean for admin-app/web (1 pre-existing prettier error in `Step4RouterMac.tsx` unchanged); type-check + format:check clean for marketing-site. | (this commit) |
| 2026-05-09 | Tabs primitive port (tenth primitive — first non-floating Radix-wrap; first slice where Forge styles inner parts via descendant selector). Survey of the five legacy call sites (`DnsStatsSection`, `TunnelThroughputChart`, `CreateTunnelInline`, `pages/Devices`, `pages/Dhcp`) found all five exclusively use the default pill (`.tabs`) surface — zero use legacy `variant="line"` or `orientation="vertical"`, and all five are *view switches inside a card or section*, never page-level navigation. So Decision 1 collapsed to "ship one primitive bound to one surface" — `.tabs` only; the legacy `variant="line"` branch and its parallel CVA dropped per "drop features the migration doesn't require"; `.tabs-bar` itself doesn't exist in `source/forge/styles.css` (only in `source/forge/docs/tailwind.config.js`) so the slice doesn't add it — if a future router-driven page-nav surface wants an underline strip, that lands in its own slice (and likely won't go through Radix Tabs at all since URL state is the source of truth). **Forge-first:** added `[data-state="active"]` next to the existing `.tabs button.is-active` selector (Toggle-pattern dual selector — third application of the data-state bridge across CSS-only mocks and React primitives); active-state visual is **static** (background + color + shadow swap, no animation declared), so no keyframes — confirms the Modal-slice intrinsic-vs-variant rule covers static state changes too. **Multi-part template:** four exports across two shapes — passthrough Radix (`Tabs`, `TabsTrigger`, `TabsContent`) and Radix-part-bearing-Forge-class (`TabsList` = `.tabs`). Forge's `.tabs button` selector targets descendants directly (no `.tabs__btn` class), so `TabsTrigger` is a passthrough — Radix Trigger renders a `<button>` by default and inherits `.tabs button` styles automatically; `TabsContent` is also a passthrough since Forge has nothing for content panels (call sites supply their own `mt-4` etc.). Tabs sits between Toggle (1/1) and Popover (3/2) on the small end of the template axis (Modal at 9/4 anchors the ceiling). Briefing prediction "4 exports across 1–2 shapes" landed exactly. **Root passthrough verification:** legacy primitive baked `flex gap-2 data-horizontal:flex-col` defaults on the Root; the new primitive drops them. Walked every call site and confirmed either (a) the default was redundant (`Devices`/`Dhcp` already pass their own `flex min-h-0 flex-1 flex-col`) or (b) layout still works because content panels are block-level (`CreateTunnelInline` — TabsList is inline-flex, TabsContent panels break onto a new line, `mt-4` provides the gap). New `./tabs` subpath export in `@wardnet/forge-web`; 5 call sites retargeted (`@/components/core/ui/tabs` → `@wardnet/forge-web/tabs`) — pure import-path swap, no JSX rewrite. Legacy `core/ui/tabs.tsx` deleted. Legacy alias audit: deletion didn't zero any rows (the deleted aliases were long-tail rows — `text-muted-foreground`, `bg-muted`, `bg-foreground`, `bg-background`, `border-input`, etc., all with many remaining consumers). **`tw-animate-css` holdout state unchanged** — `core/ui/sheet.tsx` is still the sole file carrying utility-class motion; this slice didn't touch it. Type-check + lint + build clean for admin-app/web (1 pre-existing prettier error in `Step4RouterMac.tsx` unchanged); type-check + format:check clean for marketing-site. | (this commit) |
