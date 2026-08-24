/** The subscriber's encryption keys, as produced by `PushManager.subscribe()`. */
export interface WebPushKeys {
  /** Base64url (unpadded) P-256 ECDH public key of the subscriber. */
  p256dh: string;
  /** Base64url (unpadded) shared authentication secret. */
  auth: string;
}

/** A standard Web Push subscription object. */
export interface WebPushSubscription {
  /** Push service endpoint the daemon POSTs encrypted payloads to. */
  endpoint: string;
  keys: WebPushKeys;
}

/** Response of `GET /api/push/vapid-public-key`. */
export interface VapidPublicKeyResponse {
  /**
   * Base64url (unpadded) uncompressed P-256 application server key, passed to
   * `PushManager.subscribe({ applicationServerKey })`.
   */
  key: string;
}

/**
 * Stable machine tags for push notifications, mirroring the daemon's
 * `NotificationKind` enum. Kept open (`string & {}`) so an older client keeps
 * working when the daemon ships a new kind.
 *
 * Openness is for kinds this SDK has not heard of yet — every kind it *has*
 * heard of belongs in the union, so a mistyped `case "tunnel_unhealty"` is a
 * compile error instead of dead code that silently never matches.
 */
export type NotificationKind =
  | "routing_locked"
  | "routing_unlocked"
  | "routing_changed"
  | "new_device_quarantined"
  | "access_request_created"
  | "private_dns_granted"
  // Anomaly-backed notifications carry the anomaly type's own slug, from
  // `AnomalyType::as_str` — there is no generic "anomaly" kind.
  | "tunnel_start_failed"
  | "tunnel_unhealthy"
  | "update_failed"
  | "dhcp_conflict"
  | "route_table_lost"
  | "blocklist_refresh_failing"
  // Retired: tunnel failures are anomalies now. Kept because historic feed
  // rows still carry it.
  | "tunnel_offline"
  | (string & {});

/**
 * One entry of the admin notification feed: a persisted record of an
 * admin-audience push notification.
 */
export interface NotificationItem {
  id: string;
  /** Stable machine tag, e.g. `new_device_quarantined`, `tunnel_offline`. */
  kind: NotificationKind;
  title: string;
  body: string;
  /** App-relative deep link (no PWA base path), e.g. `/devices`. */
  url?: string;
  /**
   * Identifier of the subject entity; what it identifies is driven by `kind`
   * (device UUID for device kinds, tunnel UUID for tunnel kinds).
   */
  subject_id?: string;
  /** RFC 3339 creation timestamp. */
  created_at: string;
}

/** Response of `GET /api/push/notifications`. */
export interface NotificationsResponse {
  /** Newest first. */
  notifications: NotificationItem[];
}
