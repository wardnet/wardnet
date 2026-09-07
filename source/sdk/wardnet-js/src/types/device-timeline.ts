/**
 * A device's connectivity timeline: what it *did*, as opposed to how it is
 * configured.
 */

/** What happened to a device. Open so a newer daemon can add kinds. */
export type DeviceEventKind =
  | "discovered"
  | "returned"
  | "gone"
  | "ip_changed"
  | "zone_changed"
  | "routing_changed"
  | "conntrack_flushed"
  | (string & {});

/** One observational entry on the timeline. */
export interface DeviceTimelineEvent {
  at: string;
  kind: DeviceEventKind;
  /** Kind-specific payload — the addresses of a change, a flush's reason. */
  details: Record<string, unknown> | null;
}

/** A DHCP lease event, from the lease audit trail. */
export interface DeviceDhcpEvent {
  at: string;
  event_type: "assigned" | "renewed" | "released" | "expired" | "conflict";
  details: string | null;
}

/**
 * DNS activity for one bucket, split by result.
 *
 * The split is the point: a device at one query a minute where every query
 * succeeded rules DNS out, which a total alone cannot do.
 */
export interface DeviceDnsBucket {
  at: string;
  /** Result slug (`forwarded`, `blocked`, `cache_hit`, …) to count. */
  results: Record<string, number>;
}

/** How far back a timeline request reaches. */
export type DeviceTimelineWindow =
  | "one_hour"
  | "six_hours"
  | "twenty_four_hours"
  | "seven_days";

export interface DeviceTimelineResponse {
  from: string;
  to: string;
  /** Width of each `dns` bucket: 60s for short windows, 3600s for long ones. */
  bucket_secs: number;
  events: DeviceTimelineEvent[];
  dhcp: DeviceDhcpEvent[];
  dns: DeviceDnsBucket[];
}
