import type { WardnetClient } from "../client.js";
import type { VapidPublicKeyResponse, WebPushSubscription } from "../types/push.js";

/**
 * Web Push notifications.
 *
 * All methods are accessible to both admins (session / API key) and
 * unauthenticated LAN devices — the daemon keys the subscription to the admin
 * account when a session is present, otherwise to the calling device.
 */
export class PushService {
  constructor(private readonly client: WardnetClient) {}

  /**
   * The VAPID application server public key (base64url). Pass it to
   * `PushManager.subscribe({ applicationServerKey })`. Unauthenticated.
   */
  async getVapidPublicKey(): Promise<string> {
    const res = await this.client.request<VapidPublicKeyResponse>("/push/vapid-public-key");
    return res.key;
  }

  /** Register the browser's push subscription for the calling context. */
  async subscribe(subscription: WebPushSubscription): Promise<void> {
    await this.client.request<unknown>("/push/subscriptions", {
      method: "POST",
      body: JSON.stringify(subscription),
    });
  }

  /**
   * Remove the caller's push subscription(s). Pass an `endpoint` to remove just
   * that one; omit it to remove all subscriptions the caller owns.
   */
  async unsubscribe(endpoint?: string): Promise<void> {
    const qs = endpoint ? `?endpoint=${encodeURIComponent(endpoint)}` : "";
    await this.client.request<unknown>(`/push/subscriptions${qs}`, {
      method: "DELETE",
    });
  }
}
