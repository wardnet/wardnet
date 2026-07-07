---
status: accepted
date: 2026-05-31
issue: "#439"
---

# ADR: PWAs compose existing endpoints — no BFF, enrich resources over view aggregators

---

## Context

The admin mobile PWA (#439) and the upcoming user PWA (#438) are new web
surfaces served from the same daemon as the desktop admin site. Their
screens — a summary dashboard, a device list, a tunnel status list — need
data that today is spread across several existing endpoints:

- `GET /api/system/status` — device/tunnel counts, uptime, version, CPU/mem.
- `GET /api/devices` — device list (+ DHCP status).
- `GET /api/tunnels` — tunnel list (cumulative bytes, last handshake).
- `GET /api/stats` — generic pre-aggregated time-series (e.g. `dns.queries`
  labelled by `{outcome}`, `tunnel.bytes.tx`/`.rx` labelled by `tunnel_id`).

Two recurring shapes of data don't come from a single scalar field:

1. **DNS "today" totals** (total queries, blocked queries) — derived from
   `dns.queries` summed over the day, with `outcome=blocked` for the blocked
   slice.
2. **Throughput rate** (per-tunnel and combined `KB/s`) — derived from the
   delta of the `tunnel.bytes.*` counters over a short window.

This raised the question of whether mobile should get a dedicated
backend-for-frontend (BFF): mobile-shaped aggregation endpoints (e.g.
`GET /api/dashboard`) that return one snapshot per screen.

## Decision

**The PWAs compose existing resource endpoints client-side. We do not
introduce a BFF or per-screen aggregation endpoints.** Cross-cutting fetch
concerns (parallelism, dedupe, caching, refetch-on-focus) are handled by
react-query in the shared `web` layer.

When a screen genuinely needs data the daemon doesn't expose, the default is
to **enrich the shared domain resource** — which benefits every surface —
rather than add a view-specific aggregator. The first instance: the device
list response gains the device's current routing target (#439 backend
sub-task), so the Devices screen needs one request instead of N+1, and the
desktop admin benefits too.

## Why not a BFF / dashboard endpoint

- **It wouldn't collapse the chatty screens anyway.** The dashboard's
  sparklines require `/api/stats` time-series regardless; an aggregation
  endpoint can only serve the scalar numbers, leaving the time-series calls
  in place.
- **It largely duplicates `/api/system/status`**, which already carries most
  of the dashboard's scalars.
- **It is a whole architectural layer** — view-shaped DTOs, their own
  auth/versioning, and ongoing coupling of the daemon to mobile *view*
  concerns — which cuts against the layered/trait architecture
  (`.agents/architecture.md`) and the project's "no speculative primitives"
  principle. There is no mobile-latency evidence justifying it yet.
- **The precedent already works**: the desktop admin dashboard composes
  multiple calls successfully.

## Consequences

- The dashboard and tunnels screens issue several parallel requests on load;
  react-query caching/refetch-on-focus keeps this acceptable.
- Throughput rate granularity equals the `/api/stats` bucket size.
- **Reversal trigger**: if a screen is measured to be too chatty or slow on
  real cellular, add a *single* mobile-summary endpoint at that point — the
  composed call sites make it obvious which fields belong in it. Adding such
  an endpoint later is cheap; unwinding a premature BFF is not.
- Resource enrichment (the device-list routing target) is the sanctioned way
  to close a data gap, because the enriched representation is shared by all
  surfaces rather than scoped to one screen.
