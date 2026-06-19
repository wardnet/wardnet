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
- TypeScript 5.9, zero runtime dependencies
- Uses native `fetch` (works in browser and Node 18+)
- No DOM types — minimal `globals.d.ts` for cross-environment support
- Linked via Yarn `portal:` protocol from all app surfaces (`"@wardnet/js": "portal:../sdk/wardnet-js"`)
- Yarn 4 with `nodeLinker: node-modules`

## Shared React library (`@wardnet/web`)
- Lives at `source/web/`; linked via `"@wardnet/web": "portal:../web"`
- Contains all shared TanStack Query hooks (useTunnels, useRebuildTunnel, useCombinedTunnelStats, useDevices, useStats, …), shared components (LoginForm, JobProgressDescription), Zustand stores, utility functions, and all UI primitives (Button, Card, Modal, Combobox, etc.)
- All app surfaces (admin-site, user-app, admin-app) import hooks, utilities, and primitives from here — **do not duplicate hook or component logic in app-local files**

## Design tokens (`@wardnet/styles`)
- Lives at `source/styles/`; linked via `"@wardnet/styles": "portal:../styles"`
- CSS tokens + Tailwind base layer in `styles.css`; typed design token constants (brand, status, radius, density, font) in `src/tokens.ts`
- `typography.css` — semantic text variant classes (`.t-label`, `.t-body`, `.t-metric`, `.t-h1`…) plus `t-size-*` / `t-weight-*` helpers, all in `@layer components`; variant selectors are wrapped in `:where()` (zero specificity) so helper and colour utilities override structurally. Imported from `styles.css`, so consumers reach it via the single `@wardnet/styles` CSS entry. Backs the `<Text>` primitive in `@wardnet/ui` (see `docs/adr-typography-scale-and-roles.md`).
- Import CSS: `@import "@wardnet/styles"` (the `"."` export resolves to `styles.css`)
- Import tokens: `import { brand, status } from "@wardnet/styles/tokens"`

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
- Topic catalogue in `source/site/content/docs.yml` (loaded via `@modyfi/vite-plugin-yaml`)

## PWA initiative (issues #435–#441)

- **Three app surfaces** — admin site (desktop, at `/admin/`), user PWA (at `/`), admin mobile PWA (at `/admin-app/`). All served from a single origin; independently installable via distinct `manifest.json` scopes. Admin site and admin-app are both live; user-app is still planned (issue #438). See `CONTEXT.md` for the full glossary.
- **Daemon-owned TLS** — `wardnetd` terminates TLS itself on `:443` (no Caddy, diverging from issue #436): it issues/renews its certificate natively via ACME **DNS-01** (`instant-acme` + `rcgen` + `rustls`/`axum-server`), publishing `_acme-challenge` TXT through the **DnsProvider**. `:80` redirects to HTTPS. See `docs/adr-daemon-owned-tls.md`.
- **DDNS + ACME bridge service** — wardnet-operated service assigning each install a vanity name (`<vanity>.my.wardnet.services`) and acting as ACME bridge for Let's Encrypt DNS-01 challenges. The cert private key is generated on the Pi and never leaves it. See issue #435.
- **VAPID / Web Push** — daemon-side push notification support (VAPID key pair generated at setup, subscription records keyed to device MAC or admin session). See issue #440.
