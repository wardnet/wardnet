# Technical Stack

## Daemon
- Rust 1.95 (pinned in `rust-toolchain.toml`)
- **Multi-crate workspace**: `wardnet-common` (shared types/config) → `wardnetd-data` (repositories + database dumper + secret store) → `wardnetd-services` (business logic) → `wardnetd-api` (HTTP layer) → `wardnetd` (Linux binary)
- axum 0.8 (with `macros`, `multipart`, `ws` features), tokio, tower-http
- utoipa + utoipa-axum for OpenAPI generation, utoipa-scalar for the `/api/docs` UI
- SQLite via sqlx 0.8 (runtime queries with `.bind()`, not compile-time macros)
- argon2 for password/API key hashing (Argon2id), SHA-256 for session tokens
- age (passphrase mode, scrypt + ChaCha20-Poly1305) for backup bundles
- sysinfo for host CPU/memory monitoring
- rust-embed to serve web UI from the binary
- async-trait for trait object interfaces
- `wardnetd-mock` — local dev binary: full API with no-op network backends, on-disk or in-memory SQLite, real file-backed secret store under `/tmp/wardnet-mock/secrets`

## SDK (`@wardnet/js`)
- TypeScript 5.9, zero runtime dependencies
- Uses native `fetch` (works in browser and Node 18+)
- No DOM types — minimal `globals.d.ts` for cross-environment support
- Linked to web-ui via Yarn `portal:` protocol (`"@wardnet/js": "portal:../sdk/wardnet-js"`)
- Yarn 4 with `nodeLinker: node-modules`

## Web UI
- React 19, TypeScript 5.9, Vite 7
- Tailwind CSS 4 (CSS-first config: `@import "tailwindcss"` + `@tailwindcss/vite` plugin)
- shadcn/ui (Radix UI primitives + Tailwind styling) — components in `src/components/core/ui/`
- TanStack Query 5, React Router 7, Zustand 5
- ESLint 10 + Prettier
- Yarn 4 with `nodeLinker: node-modules`
- Path alias: `@/` → `src/` (Vite + tsconfig)

## Public site
- Same stack as the web UI (React 19 + Vite + Tailwind 4)
- Docs are plain markdown under `source/site/content/docs/`, rendered via `react-markdown` + `remark-gfm` with custom component mappings in `DocsArticle.tsx`
- Topic catalogue in `source/site/content/docs.yml` (loaded via `@modyfi/vite-plugin-yaml`)

## Planned infrastructure (PWA initiative — issues #435–#441)

- **Caddy** — reverse proxy bundled in the release tarball alongside `wardnetd`. Runs as a companion systemd service. Handles TLS termination on port 443 and forwards to the daemon on port 7411. The daemon manages the Caddyfile on startup. See issue #436.
- **DDNS + ACME bridge service** — wardnet-operated service assigning each install a subdomain (`<id>.wardnet.network`) and acting as ACME bridge for Let's Encrypt DNS-01 challenges. The cert private key is generated on the Pi and never leaves it. See issue #435.
- **VAPID / Web Push** — daemon-side push notification support (VAPID key pair generated at setup, subscription records keyed to device MAC or admin session). See issue #440.
- **Three app surfaces** — admin site (desktop, at `/admin/`), user PWA (at `/`), admin mobile PWA (at `/admin-app/`). All served from a single origin; independently installable via distinct `manifest.json` scopes. See `CONTEXT.md` for the full glossary and issues #437–#439 for implementation.
