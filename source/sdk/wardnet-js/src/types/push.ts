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
