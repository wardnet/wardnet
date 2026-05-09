# Wardnet Forge — Design System

**Wardnet Forge** is the design system behind Wardnet, a network OS for technical homes (router, DHCP, DNS, WireGuard tunnels, content filtering, observability). One system, three surfaces: **web admin**, **mobile companion**, and an **embedded HUD** for the device itself.

---

## Files

| File | What it is |
|---|---|
| `index.html` | Desktop admin prototype (all 7 sections + 2 detail screens) |
| `mobile.html` | Mobile companion app (4 device frames, both themes) |
| `design-system.html` | This system, visualized: tokens, type, components, voice |
| `data.jsx` | Mock data (devices, tunnels, leases, logs, etc.) |
| `primitives.jsx` | Shared React primitives (`Icon`, `Sparkline`, `Donut`, `Toggle`, `StatTile`) |
| `screens.jsx` / `detail-screens.jsx` | Section + detail screens |
| `app.jsx` | App shell (sidebar, header, theme + density + accent) |
| `tweaks-panel.jsx` | Theme / density / sidebar style / accent picker |

> **Note:** This directory is the design-system *studio* — visual reference
> and mocks. The canonical artifacts now live one level up:
> - `../styles.css` — Forge CSS (`@wardnet/forge`'s web manifestation)
> - `../src/tokens.ts` — platform-neutral token values
>
> React primitives that wrap Radix in Forge classes live in
> `source/admin-app/forge-web/` (`@wardnet/forge-web`) — they're internal to
> the admin product. The studio mocks here still reference `../styles.css`
> for standalone HTML rendering; long-term the mocks will render against the
> real package builds.

---

## Brand

**Wardnet** — guard + net. Quiet, dependable, technical. The brand is the shield logo + signal-green accent on Ward Navy.

- **Brand colors** — Ward Navy `#0C1230` · Signal Green `#1ED68A` (`oklch(0.83 0.18 158)`)
- **Type pairing** — Inter Tight (UI / display) · JetBrains Mono (IPs, MACs, ports, logs)
- **Logo** — square shield, navy fill, green signal mark; rendered at 26px in chrome, 60px+ in marketing
- **Tone** — direct, technical, never breathless. See *Voice* in `design-system.html`.

---

## The six principles

1. **Numbers earn the room.** Stats are the loudest thing on a page.
2. **Status before chrome.** Every surface answers "is it healthy?" within 200ms of load.
3. **Mono for facts.** IPs, MACs, ports, hashes, durations — all monospaced.
4. **Surfaces, not pages.** The shell never moves. Content swaps inside.
5. **Dense without crowded.** Comfortable density baked in — `--pad: 18px`, `--row-h: 52px`. No toggle.
6. **Action lives on the right.** Cancel left, primary right. Delete is always red, always last.

---

## Consuming Forge from an app

Forge's CSS lives in `design-system/styles.css` at the repo root and is the **single source of truth** — both `source/web-ui` and `source/site` consume it directly via a Vite alias rather than vendoring a copy. When tokens change in Forge, both apps pick up the change on the next dev-server reload; there's no drift to babysit.

### Setup (per app)

```ts
// vite.config.ts
import { fileURLToPath } from "node:url";

resolve: {
  alias: {
    "@wardnet/forge": fileURLToPath(
      new URL("../../design-system", import.meta.url)
    ),
  },
}
```

```css
/* src/index.css */
@import "tailwindcss";
@import "@wardnet/forge/styles.css";
@import "@fontsource-variable/inter-tight";
@import "@fontsource-variable/jetbrains-mono";

@custom-variant dark (&:where([data-theme="dark"], [data-theme="dark"] *));

@theme inline {
  /* mirror Forge tokens into Tailwind utilities — see styles.css */
}
```

Then set `attribute="data-theme"` on `next-themes` so Forge's `[data-theme="dark"]` block fires:

```tsx
<ThemeProvider attribute="data-theme" defaultTheme="system" enableSystem>
```

**Why a Vite alias and not vendoring:** vendoring forks the file. The first time tokens change in Forge, someone has to remember to copy the file into both apps — the kind of step that gets skipped for "small fixes" until the surfaces drift visibly. The alias makes Forge structurally load-bearing: changing it changes the apps in lockstep, which is the property we want.

**Why not publish Forge as an npm package:** the apps live in the same repo as Forge; the alias is zero-config and zero-publish-cycle. If we ever extract Forge to its own repo, this swaps to a package import without changing the consuming code.

---

## Token system

All tokens live in `styles.css` under `:root` (light) and `[data-theme="dark"]`. Comfortable density is baked in — no toggle.

### Colors
- **Surfaces** — `--bg`, `--bg-elev`, `--bg-sunken`, `--bg-card`, `--line`, `--line-strong`
- **Ink** — `--ink`, `--ink-2`, `--ink-3`, `--ink-4`
- **Sidebar** — `--side-bg`, `--side-line`, `--side-ink`, `--side-active-bg`
- **Brand / status** — `--accent`, `--warn`, `--danger`, `--info` (each with `-soft` and `-soft-ink` variants)

### Type
`--font-sans: "Inter Tight"` · `--font-mono: "JetBrains Mono"`. Eight named scales: Display, H1, H2, Stat (tabular), Body, Label, Mono Body, Mono Log.

### Spacing & radius
4-step modular: 4, 8, 12, 16, 24, 32, 48. Radii: 6 / 10 / 14 / 20 / pill.

### Elevation
Two shadows only — `--shadow-card` (resting) and `--shadow-pop` (popovers, modals).

---

## Surfaces

### Desktop admin (`index.html`)
- **Floating sidebar** as the only variant we ship, 252px
- **Pages** swap inside the canvas; sidebar never moves
- **Dashboard** — 9-tile grid, sparkline trends, live log stream, recent errors
- **Detail screens** — Tunnel detail with throughput chart + connected devices; Device detail with inline-edit panels (Identity / Settings / DNS filtering / Network)
- **Modals** — Add tunnel (paste WireGuard config or fill form)
- **Tweaks** — light/dark theme only (density / sidebar style / accent are locked)

### Mobile (`mobile.html`)
Same tokens, rebuilt around a thumb:
- **Bottom tab bar** (5 tabs): Home, Devices, Tunnels, Filter, More
- **Hero card** on Home with status badge + headline number
- **List rows** at 52px
- **Sheets, not modals** (slide-up)
- Single-column lists everywhere

### Embedded HUD
Reserved. Same token set, kiosk-mode dark, no sidebar — single screen rotates between Network · Tunnels · Filtering.

---

## Voice rules

| Do | Don't |
|---|---|
| State + number + window: *"Filter is on. 4,747 of 1.25M queries blocked today."* | *"Your DNS filtering is currently active and protecting your network."* |
| Name the subject: *"Revoke lease for Galaxy S24 Pedro?"* | *"Are you sure you want to remove this device?"* |
| Verb-first actions: *"Bring up United States"* | *"Connect to United States now"* |

No emoji. No exclamation marks outside marketing. "Currently" is banned.

---

## Motion

Three durations only: **120ms** snap (hover, focus, toggle), **200ms** slide (sheets, popovers, dropdowns), **600ms** stream (sparkline draw, live indicator pulse, log row enter). Easing: `cubic-bezier(.2, .8, .2, 1)`.
