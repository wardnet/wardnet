/// <reference lib="webworker" />
// Service-worker push handlers shared by the Wardnet PWAs (issues #482/#764).
//
// This module must stay window-free (no imports from the package barrel — the
// SDK singletons touch `window` at module scope). Consumed via the
// `@wardnet/web/sw` subpath export from each app's `src/sw.ts`.

import type { PushNotificationData, PushPayload } from "./pushPayload";

export interface PushHandlerOptions {
  /**
   * Notification icon, resolved by the browser against the SW scope when
   * relative (e.g. `"icons/admin-192.png"`).
   */
  icon?: string;
  /**
   * Android status-bar badge: a monochrome white-on-transparent PNG
   * (e.g. `"icons/badge-96.png"`). Without it Android falls back to a
   * generic browser glyph. Ignored on platforms without badge support.
   */
  badge?: string;
}

/**
 * Resolve a daemon-provided app-relative deep link (e.g. `/devices`) against
 * the service worker's registration scope, so the same payload works for any
 * PWA base path (`https://host/admin-app/` → `https://host/admin-app/devices`).
 * Absent/empty URLs land on the scope root.
 */
export function resolveClickUrl(
  url: string | undefined,
  scope: string,
): string {
  return new URL((url ?? "/").replace(/^\//, ""), scope).href;
}

/**
 * Whether a window client belongs to this app. The scope always ends in a
 * slash, but the daemon also serves the app shell at the slashless URL
 * (`/admin-app`), so a plain `startsWith(scope)` would miss a window sitting
 * exactly there and open a duplicate.
 */
export function isInScope(clientUrl: string, scope: string): boolean {
  if (clientUrl.startsWith(scope)) return true;
  const withoutSlash = scope.replace(/\/$/, "");
  const path = clientUrl.split(/[?#]/, 1)[0];
  return path === withoutSlash;
}

/**
 * Attach `push` and `notificationclick` handlers to a service worker.
 *
 * `push` shows the daemon's `{title, body, data}` payload as a notification,
 * collapsing repeats of the same subject (`kind` + `subject_id`) via the
 * notification tag while keeping distinct subjects — two different tunnels
 * going offline — as separate notifications. `notificationclick` focuses an
 * existing app window (or opens one) on the payload's deep link.
 */
export function registerPushHandlers(
  sw: ServiceWorkerGlobalScope,
  options: PushHandlerOptions = {},
): void {
  sw.addEventListener("push", (event) => {
    let payload: PushPayload | undefined;
    try {
      payload = event.data?.json() as PushPayload | undefined;
    } catch {
      return; // Not JSON — not one of ours.
    }
    if (!payload?.title) return;
    const data = payload.data;
    event.waitUntil(
      sw.registration.showNotification(payload.title, {
        body: payload.body,
        tag: data ? `${data.kind}:${data.subject_id ?? ""}` : undefined,
        icon: options.icon,
        badge: options.badge,
        data: data ?? {},
      }),
    );
  });

  sw.addEventListener("notificationclick", (event) => {
    event.notification.close();
    const data = event.notification.data as PushNotificationData | undefined;
    const target = resolveClickUrl(data?.url, sw.registration.scope);
    event.waitUntil(
      (async () => {
        const clients = await sw.clients.matchAll({
          type: "window",
          includeUncontrolled: true,
        });
        const existing = clients.find((c) =>
          isInScope(c.url, sw.registration.scope),
        );
        if (existing) {
          await existing.focus();
          try {
            await existing.navigate(target);
            return;
          } catch {
            // navigate() rejects on windows this SW does not control (e.g. a
            // tab opened before the first SW activation — we never call
            // clients.claim()). Fall through to opening a fresh window so the
            // deep link still lands.
          }
        }
        await sw.clients.openWindow(target);
      })(),
    );
  });
}
