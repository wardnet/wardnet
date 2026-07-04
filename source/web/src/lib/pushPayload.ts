// Wire types for the daemon's Web Push payload and the small helpers shared
// by the window-side subscribe flow and the service-worker handlers. This
// module must stay window-free: it is imported by service workers via the
// `@wardnet/web/sw` subpath export.

import type { NotificationKind } from "@wardnet/js";

/**
 * Structured companion to the human title/body, produced by the daemon.
 * The service worker collapses notifications by `kind` + `subject_id` and
 * deep-links via `url` on click.
 */
export interface PushNotificationData {
  /** Stable machine tag, e.g. `new_device_quarantined`, `tunnel_offline`. */
  kind: NotificationKind;
  /**
   * App-relative deep link (no PWA base path), e.g. `/devices`. Resolved by
   * the service worker against its own registration scope.
   */
  url?: string;
  /**
   * Identifier of the subject entity; what it identifies is driven by `kind`
   * (device UUID for device kinds, tunnel UUID for tunnel kinds).
   */
  subject_id?: string;
}

/** The JSON body of a daemon push message. */
export interface PushPayload {
  title: string;
  body: string;
  data?: PushNotificationData;
}

/**
 * Decode a base64url (unpadded) string into the `Uint8Array` that
 * `PushManager.subscribe({ applicationServerKey })` expects. The daemon's
 * VAPID public key endpoint returns base64url.
 */
export function urlBase64ToUint8Array(base64Url: string): Uint8Array {
  const padding = "=".repeat((4 - (base64Url.length % 4)) % 4);
  const base64 = (base64Url + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = atob(base64);
  const output = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) {
    output[i] = raw.charCodeAt(i);
  }
  return output;
}
