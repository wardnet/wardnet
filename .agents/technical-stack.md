# Technical Stack

## Daemon
- Rust 1.96 (pinned in `rust-toolchain.toml`)
- **Multi-crate workspace**: `wardnet-common` (shared types/config) → `wardnetd-data` (repositories + database dumper + secret store) → `wardnetd-services` (business logic) → `wardnetd-api` (HTTP layer) → `wardnetd` (Linux binary)
- axum 0.8 (with `macros`, `multipart`, `ws` features), tokio, tower-http
- utoipa + utoipa-axum for OpenAPI generation, utoipa-scalar for the `/api/docs` UI
- SQLite via sqlx 0.8 (runtime queries with `.bind()`, not compile-time macros)
- argon2 for password/API key hashing (Argon2id), SHA-256 for session tokens
- age (passphrase mode, scrypt + ChaCha20-Poly1305) for backup bundles
- sysinfo for host CPU/memory monitoring
- rust-embed to serve web UI from the binary
- async-trait for trait object interfaces
- ed25519-dalek for Ed25519 key generation and request signing (DDNS bridge registration and signed IP-update calls)
- wiremock (dev-only) — mock HTTP server used in DDNS provider integration tests (bridge + Cloudflare)
- `wardnetd-mock` — local dev binary: full API with no-op network backends, on-disk or in-memory SQLite, real file-backed secret store under `/tmp/wardnet-mock/secrets`

## SDK (`@wardnet/js`)
- TypeScript 5.9, zero required runtime dependencies
- Logging ships a zero-dependency console adapter by default; `consola` is an **optional** peer dependency — consumers that want richer output enable it via `setAdapter(createConsolaAdapter())` from the `@wardnet/js/consola` subpath
- Uses native `fetch` (works in browser and Node 18+)
- No DOM types — minimal `globals.d.ts` for cross-environment support
- Linked via Yarn `portal:` protocol from all app surfaces (`"@wardnet/js": "portal:../sdk/wardnet-js"`)
- Yarn 4 with `nodeLinker: node-modules`

## Shared React library (`@wardnet/web`)
- Lives at `source/web/`; linked via `"@wardnet/web": "portal:../web"`
- Contains all shared TanStack Query hooks (useTunnels, useRebuildTunnel, useCombinedTunnelStats, useDevices, useStats, …), shared components (LoginForm, JobProgressDescription), Zustand stores, and utility functions
- **UI primitives/components live in `@wardnet/ui`** (see below); `@wardnet/web` re-exports them (`export * from "@wardnet/ui"`) so existing consumers can keep importing primitives from `@wardnet/web`. New surfaces should import design-system components directly from `@wardnet/ui`.
- All app surfaces (admin-site, user-app, admin-app) import hooks, utilities, and primitives from here — **do not duplicate hook or component logic in app-local files**

## Design system (`@wardnet/ui`, `@wardnet/styles`)

**These are external, versioned dependencies — not first-party code in this repo.**
They are published to GitHub Packages from the sibling `wardnet-design-system`
repository and consumed here as ordinary npm deps (`@wardnet/ui`, `@wardnet/styles`,
`@wardnet/brand`). There is no `source/ui/` or `source/styles/` directory in this
tree; changing a component or a token means a release in that repo followed by a
**dependency bump here**, not an edit.

- `@wardnet/ui` — all UI primitives and components (Button, Card, Modal, Combobox, Drawer, Select, Toggle, Sparkline, SegmentedTabs, FormActions, StatTile, the `<Text>`/`<Heading>` typography primitives, …), styled with CSS Modules plus `@wardnet/styles` tokens.
- `@wardnet/styles` — CSS tokens + Tailwind base layer, and `typography.css`, which backs the `<Text>` primitive (see `docs/adr/0012-typography-scale-and-roles.md`).
- Import CSS: `@import "@wardnet/styles"`. Import tokens: `import { brand, status } from "@wardnet/styles/tokens"`.

## Admin site (`source/admin-site/` — `@wardnet/admin-site`)
- Full desktop admin UI; served at `/admin/`
- React 19, TypeScript 5.9, Vite 7
- Tailwind CSS 4 (CSS-first config: `@import "tailwindcss"` + `@tailwindcss/vite` plugin)
- shadcn/ui (Radix UI primitives + Tailwind styling) — components in `src/components/core/ui/`
- TanStack Query 5, React Router 7, Zustand 5
- ESLint 10 + Prettier
- Yarn 4 with `nodeLinker: node-modules`
- Path alias: `@/` → `src/` (Vite + tsconfig)

## Admin mobile PWA (`source/admin-app/` — `@wardnet/admin-app`)
- Admin PWA for daily operational tasks; served at `/admin-app/`
- React 19, TypeScript 5.9, Vite 7
- Tailwind CSS 4
- TanStack Query 5, React Router 7, Zustand 5, Sonner (toasts)
- `OnlineStatusContext` — provides `showingLastKnownState: boolean` to all pages; pages wrap content in an offline overlay (pointer-events disabled + opacity dimmed) when true
- Yarn 4 with `nodeLinker: node-modules`
- Path alias: `@/` → `src/` (Vite + tsconfig)

## Public site
- Same stack as the web UI (React 19 + Vite + Tailwind 4)
- Docs are plain markdown under `source/site/content/docs/`, rendered via `react-markdown` + `remark-gfm` with custom component mappings in `DocsArticle.tsx`
- Topic catalogue in `source/site/content/docs.yml` (loaded via `@modyfi/vite-plugin-yaml`); slugs without a matching markdown file render as "coming soon"
- `/docs/api-reference` (`ApiReference.tsx`) is a multi-version OpenAPI viewer: `yarn generate:release-manifests` downloads each distinct release's `openapi.json` into `public/api-specs/<sha>.json` and records `spec_path` in the release manifest; a daemon-version picker renders the matching spec via **Scalar**, themed with Forge design tokens through Scalar's `customCss` (forced light mode, site fonts) and made read-only (`hideTestRequestButton`/`hideClientButton` — there's no live daemon to authenticate against from the public site). The Scalar bundle itself is vendored from the daemon at build time by `yarn copy:scalar` (copies `source/daemon/crates/wardnetd-api/assets/vendor/scalar-api-reference.js` to `public/api-docs/scalar.js`). Both `public/api-specs/` and `public/api-docs/` are build-time-generated and gitignored, same as `public/releases/`.

## PWA initiative (issues #435–#441)

- **Three app surfaces** — admin site (desktop, at `/admin/`), user PWA (at `/app/`; the bare root `/` 308-redirects there), admin mobile PWA (at `/admin-app/`). All served from a single origin; the two PWAs are installable side by side because their `manifest.json` scopes are siblings, not nested (Chrome refuses to install an app whose page sits inside an installed app's scope). See `CONTEXT.md` for the full glossary.
- **Daemon-owned TLS** — `wardnetd` terminates TLS itself on `:443` (no Caddy, diverging from issue #436): it issues/renews its certificate natively via ACME **DNS-01** (`instant-acme` + `rcgen` + `rustls`/`axum-server`), publishing `_acme-challenge` TXT through the **DnsProvider**. `:80` redirects to HTTPS. See `docs/adr/0008-daemon-owned-tls.md`.
- **DDNS + ACME bridge service** — wardnet-operated service assigning each install a vanity name (`<vanity>.my.wardnet.services`) and acting as ACME bridge for Let's Encrypt DNS-01 challenges. The cert private key is generated on the Pi and never leaves it. See issue #435.
- **VAPID / Web Push** — daemon-side push notification support (VAPID key pair generated at setup, subscription records keyed to device MAC or admin session). See issue #440.
