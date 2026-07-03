/**
 * Network Zones (epic #244, issue #735).
 *
 * A Network Zone is a named policy bucket a device belongs to (exactly one). It
 * gates the device's allowed routing targets, its reachability of the Pi's admin
 * surfaces, and (Phase 2+) its network isolation. Distinct from the DNS
 * *authoritative local zone* (`types/dns-local`).
 */

/** A permitted routing-target kind. Coarse: any tunnel, or direct. */
export type AllowedTargetKind = "direct" | "tunnel";

/** Cross-zone isolation rung of the guarantee ladder. VLAN is a non-goal. */
export type ZoneStance = "shared_subnet" | "isolate_members";

/** Whether a zone was seeded by the daemon or created by an admin. */
export type ZoneProvenance = "system" | "manual";

/** A per-zone subnet (Phase 2 / DHCP-mode). Recorded-only in Phase 1. */
export interface ZoneSubnet {
  /** CIDR, e.g. "10.44.0.0/24". */
  cidr: string;
}

/** A Network Zone. */
export interface NetworkZone {
  id: string;
  name: string;
  provenance: ZoneProvenance;
  isolation_stance: ZoneStance;
  /** Routing-target kinds a device in this zone may pick. Never empty. */
  allowed_targets: AllowedTargetKind[];
  /** Within an isolate-members zone, also isolate same-zone peers. */
  member_isolation: boolean;
  /** Per-zone subnet. `null` for all seeds. */
  subnet: ZoneSubnet | null;
  /** May devices in this zone reach the Pi's admin surfaces? */
  admin_ui_reachable: boolean;
  /** The protected "home"/anchor zone. Exactly one true. Deletion-guarded. */
  is_default: boolean;
  /** Where freshly-discovered devices are assigned. Exactly one true. */
  is_default_for_new: boolean;
  created_at: string;
  updated_at: string;
}

/** A zone plus its current member count (list/detail view). */
export interface NetworkZoneView extends NetworkZone {
  member_count: number;
}

/**
 * Minimal zone info the daemon attaches to `GET /api/devices/me` for the
 * read-only zone display in the user PWA. The caller is device-keyed and can't
 * read the admin-only zone catalog, so this is the only zone data it sees.
 */
export interface ZoneSummary {
  /** Human-readable zone name (e.g. "Guest"). */
  name: string;
  /** Whether this is the protected anchor "home" zone. */
  is_default: boolean;
}
