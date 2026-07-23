/**
 * Private DNS — encrypted DNS at home and while roaming (issues #910/#914).
 *
 * An admin grants a managed device a secret hostname; the phone points Android
 * Private DNS (DoT) or an iOS configuration profile at it and resolves through
 * the box everywhere. Admin operations manage the feature and grants; the
 * device-keyed `me` surface (identified by source IP, like `devices/me`) backs
 * the user PWA.
 */

/**
 * Enable prerequisites, surfaced so the admin UI can explain a blocked toggle.
 *
 * The first three are hard gates — enabling requires all `true`.
 * `relay_connected` is informational (the LAN path works without the reverse
 * tunnel; only roaming needs it), so it never blocks enable.
 */
export interface PrivateDnsPrerequisites {
  /** The box is on the wardnet DDNS provider. */
  wardnet_provider: boolean;
  /** An issued TLS certificate is live. */
  certificate: boolean;
  /** The box holds an active Premium entitlement. */
  entitled: boolean;
  /** The reverse tunnel to the regional edge is up. Informational only. */
  relay_connected: boolean;
}

/** One per-device grant, with the full minted hostname the phone dials. */
export interface PrivateDnsGrantSummary {
  id: string;
  device_id: string;
  /**
   * The device's secret hostname (`<token>.<domain>`) — entered into Android
   * Private DNS and carried as the `ServerName` in the iOS profile.
   */
  hostname: string;
  /** RFC 3339 creation timestamp. */
  created_at: string;
}

/** Response for `GET /api/private-dns/status` and `PUT /api/private-dns`. */
export interface PrivateDnsStatusResponse {
  /** Whether the feature (and therefore the `:853` listener) is enabled. */
  enabled: boolean;
  /** The wardnet FQDN grants live under, or `null` when not on the provider. */
  domain: string | null;
  prerequisites: PrivateDnsPrerequisites;
  /** Every grant, oldest first. */
  grants: PrivateDnsGrantSummary[];
}

/** Request body for `PUT /api/private-dns`. */
export interface SetPrivateDnsEnabledRequest {
  enabled: boolean;
}

/** Request body for `POST /api/private-dns/grants`. */
export interface CreatePrivateDnsGrantRequest {
  /** The device to grant encrypted-DNS access. */
  device_id: string;
}

/**
 * Response for `POST /api/private-dns/grants/{device_id}/notify` — a device-keyed
 * push nudging the household member to set up Private DNS on their phone.
 */
export interface SendPrivateDnsNotificationResponse {
  /**
   * Whether at least one push subscription for the device was targeted. `false`
   * (still a 200) means the device holds a grant but hasn't enabled
   * notifications yet — the admin should relay the hostname directly instead.
   */
  delivered: boolean;
}

/**
 * Response for `GET /api/private-dns/me` — the device-keyed self-service view
 * (identified by source IP, like `devices/me`).
 */
export interface PrivateDnsMeResponse {
  /** Whether Private DNS is enabled on the box at all. */
  enabled: boolean;
  /** Whether this device holds a grant. */
  granted: boolean;
  /** This device's full hostname when granted; `null` otherwise. */
  hostname: string | null;
}
