import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type { NotificationItem, WebPushSubscription } from "../types/push.js";

/**
 * Web Push notifications.
 *
 * All methods are accessible to both admins (session / API key) and
 * unauthenticated LAN devices — the daemon keys the subscription to the admin
 * account when a session is present, otherwise to the calling device.
 */
export class PushService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /**
   * The VAPID application server public key (base64url). Pass it to
   * `PushManager.subscribe({ applicationServerKey })`. Unauthenticated.
   */
  async getVapidPublicKey(): Promise<string> {
    const res = await this.api.get("/push/vapid-public-key");
    return res.key;
  }

  /** Register the browser's push subscription for the calling context. */
  async subscribe(subscription: WebPushSubscription): Promise<void> {
    await this.api.post("/push/subscriptions", { body: subscription });
  }

  /**
   * Remove the caller's push subscription(s). Pass an `endpoint` to remove just
   * that one; omit it to remove all subscriptions the caller owns.
   */
  async unsubscribe(endpoint?: string): Promise<void> {
    await this.api.del("/push/subscriptions", { query: { endpoint } });
  }

  /**
   * The admin notification feed, newest first. Admin only. `limit` is clamped
   * server-side to 1..=100 (default 50).
   */
  async listNotifications(limit?: number): Promise<NotificationItem[]> {
    const res = await this.api.get("/push/notifications", { query: { limit } });
    return res.notifications;
  }

  /**
   * Clear the admin notification feed. Admin only. The feed is shared across
   * admin accounts, so this clears it for every admin.
   */
  async clearNotifications(): Promise<void> {
    await this.api.del("/push/notifications");
  }
}
