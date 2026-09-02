# Architecture Decision Records

Sequentially numbered per [`.claude/skills/challenge/ADR-TEMPLATE.md`](../../.claude/skills/challenge/ADR-TEMPLATE.md).
To add a new one, scan this directory for the highest number and increment by one,
**and add a row below in the same change** — the table is the only place the set is
listed, so an ADR missing from it is effectively unfindable.
Each file's `status`/`date`/`issue` (and `supersedes`/`superseded_by` where relevant)
live in a YAML frontmatter block at the top of the file.

Rows are ordered by number, not by date. Numbers are identifiers, and one has
already been reassigned: two ADRs were authored concurrently as `0023`, and the
later of the two (Switchback) moved to `0026` rather than churn the dozen
inbound references — including a generated SDK file — that pointed at the
edge-release-channel one.

| # | Title | Date | Status |
|---|---|---|---|
| [0001](0001-chart-zoom.md) | Drop Recharts Brush in favour of drag-to-zoom | 2026-05-27 | Accepted |
| [0002](0002-pwa-data-fetching.md) | PWAs compose existing endpoints — no BFF | 2026-05-31 | Accepted |
| [0003](0003-serving-identity-boundary.md) | TLS serving identity is a method-exposed projection | 2026-06-05 | Accepted |
| [0004](0004-global-naming-authority.md) | Global naming authority is a strongly-consistent Postgres registry | 2026-06-07 | Accepted |
| [0005](0005-two-domain-strategy.md) | Two-domain strategy — trusted brand zone vs. untrusted user-content zone | 2026-06-07 | Accepted |
| [0006](0006-bridge-edge-topology.md) | Bridge edge topology — Caddy-l4 on the front, bridge as passthrough router | 2026-06-07 | Superseded by [ADR-0007](0007-bridge-self-terminated-tls.md) |
| [0007](0007-bridge-self-terminated-tls.md) | Bridge self-terminated TLS — drop Caddy, own the edge in-process | 2026-06-09 | Accepted — supersedes [ADR-0006](0006-bridge-edge-topology.md) |
| [0008](0008-daemon-owned-tls.md) | Daemon-owned TLS termination — native ACME, no Caddy on the Pi | 2026-06-09 | Accepted |
| [0009](0009-provider-based-ddns.md) | Provider-based, daemon-owned DDNS + ACME | 2026-06-09 | Accepted |
| [0010](0010-premium-tier-and-entitlement.md) | Premium tier and entitlement model | 2026-06-13 | Accepted — decision #2 superseded by enrollment-code flow (see CONTEXT.md); decision #4 superseded by [ADR-0016](0016-daemon-cloud-auth.md) |
| [0011](0011-service-decomposition.md) | Bridge service decomposition — tenants (global) / DDNS + Tunneler (regional) | 2026-06-13 | Accepted |
| [0012](0012-typography-scale-and-roles.md) | Typography scale and semantic text roles | 2026-06-19 | Accepted |
| [0013](0013-nftables-pure-netlink.md) | nftables management via pure netlink (rustables) | 2026-06-23 | Accepted |
| [0014](0014-watchdog-and-health.md) | Three-layer watchdog + health-monitor subsystem | 2026-06-25 | Accepted |
| [0015](0015-e2e-selector-convention.md) | `data-testid`-primary selectors for the web-ui Playwright suite | 2026-06-27 | Accepted |
| [0016](0016-daemon-cloud-auth.md) | Daemon cloud auth — tenants-minted JWT + Ed25519 PoP | 2026-06-29 | Accepted — supersedes decision #4 of [ADR-0010](0010-premium-tier-and-entitlement.md) |
| [0017](0017-per-service-cloud-clients.md) | Per-service cloud clients with independent endpoints | 2026-06-29 | Accepted |
| [0018](0018-network-zone-isolation.md) | Network Zone isolation — the guarantee ladder, coarse target gating | 2026-07-01 | Accepted |
| [0019](0019-network-zone-enforcement.md) | Network Zone packet enforcement — a decoupled nftables enforcer | 2026-07-01 | Accepted |
| [0020](0020-push-notifications.md) | Push notifications — VAPID + Web Push delivery | 2026-07-01 | Accepted |
| [0021](0021-network-zone-deep-isolation.md) | Network Zone deep isolation — per-zone subnets, whole-chain L3 enforcer | 2026-07-02 | Accepted |
| [0022](0022-inbound-wireguard-and-published-access.md) | Inbound WireGuard peers are Devices; published access defaults to tunnel-only | 2026-07-07 | Accepted — decision #2 superseded by [ADR-0030](0030-published-apps.md) |
| [0023](0023-edge-release-channel.md) | An edge release channel for unvetted, on-demand builds | 2026-07-14 | Accepted |
| [0024](0024-domain-routing-profiles.md) | Domain-based routing via routing profiles | 2026-07-20 | Accepted |
| [0025](0025-device-identification.md) | Device identification — a shared vendor catalog, hedged guesses, and no background probing | 2026-08-03 | Accepted |
| [0026](0026-switchback-and-cross-zone-return.md) | Switchback — cross-zone exceptions reaching a tunnel-bound device | 2026-07-19 | Accepted |
| [0027](0027-e2e-auto-update-version-skew.md) | The auto-update e2e synthesises its version skew from one source tree | 2026-08-08 | Accepted |
| [0028](0028-shutdown-teardown-and-uninstall.md) | Runtime-state teardown on shutdown, gated on stop vs restart; `wardnetd uninstall` | 2026-08-08 | Accepted |
| [0029](0029-private-dns-dot.md) | Private DNS is a closed `DoT` resolver keyed by a per-device secret hostname | 2026-08-09 | Accepted |
| [0030](0030-published-apps.md) | A published app is a name, a reach ladder, and a policy — not a port forward | 2026-08-10 | Accepted — supersedes §2 of [ADR-0022](0022-inbound-wireguard-and-published-access.md) |
| [0031](0031-household-identity.md) | Household identity is box-local; the cloud never vouches, and device affinity never authenticates | 2026-08-10 | Accepted |
| [0032](0032-managed-devices-and-retention.md) | `managed` is an explicit latching column; only unmanaged devices are pruned | 2026-08-10 | Accepted |
| [0033](0033-household-access-requests.md) | One access-request inbox, with per-kind approvers | 2026-08-14 | Accepted — supersedes §5 of [ADR-0029](0029-private-dns-dot.md) in part |
| [0034](0034-query-log-normalisation.md) | The DNS query log is normalised onto integer lookup ids | 2026-09-02 | Accepted |
