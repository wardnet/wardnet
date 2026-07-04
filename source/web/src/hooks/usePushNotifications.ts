import { useCallback, useEffect, useState } from "react";
import { toast } from "@wardnet/ui";
import type { WebPushSubscription } from "@wardnet/js";
import { pushService } from "../lib/sdk";
import { urlBase64ToUint8Array } from "../lib/pushPayload";

/**
 * Where the caller stands with Web Push:
 * - `unsupported` — no SW / PushManager / Notification API in this browser
 *   (includes iOS Safari outside an installed PWA).
 * - `denied` — the user has blocked notifications; only a browser-settings
 *   change can undo this.
 * - `prompt` — permission not asked yet.
 * - `unsubscribed` — permission granted but no active subscription.
 * - `subscribed` — an active subscription is registered with the daemon.
 */
export type PushPermissionState =
  "unsupported" | "denied" | "prompt" | "subscribed" | "unsubscribed";

/**
 * How long to wait for an active service worker before giving up.
 * `navigator.serviceWorker.ready` never settles when no SW is registered on
 * the surface (e.g. the vite dev server, which disables the PWA plugin) —
 * without a timeout the toggle would hang busy forever with no feedback.
 */
const SW_READY_TIMEOUT_MS = 4_000;

/** The active SW registration, or `null` if none shows up within the timeout. */
function swReady(): Promise<ServiceWorkerRegistration | null> {
  return Promise.race([
    navigator.serviceWorker.ready,
    new Promise<null>((resolve) => {
      setTimeout(() => resolve(null), SW_READY_TIMEOUT_MS);
    }),
  ]);
}

function pushSupported(): boolean {
  return (
    typeof navigator !== "undefined" &&
    "serviceWorker" in navigator &&
    typeof window !== "undefined" &&
    "PushManager" in window &&
    "Notification" in window
  );
}

/** The daemon-facing subscription body for a browser `PushSubscription`. */
function toWireSubscription(sub: PushSubscription): WebPushSubscription | null {
  const json = sub.toJSON();
  if (!json.endpoint || !json.keys?.p256dh || !json.keys?.auth) return null;
  return {
    endpoint: json.endpoint,
    keys: { p256dh: json.keys.p256dh, auth: json.keys.auth },
  };
}

/**
 * Web Push subscription lifecycle for the calling context (issue #482).
 *
 * On mount, derives the current state and — when a browser subscription
 * already exists — re-registers it with the daemon (the daemon upserts by
 * endpoint, so this heals server-side loss idempotently).
 *
 * `subscribe()` must be called from a user gesture (a click handler):
 * `Notification.requestPermission()` requires one.
 */
export function usePushNotifications(): {
  state: PushPermissionState;
  isBusy: boolean;
  subscribe: () => Promise<void>;
  unsubscribe: () => Promise<void>;
} {
  const [state, setState] = useState<PushPermissionState>(() =>
    pushSupported() ? "prompt" : "unsupported",
  );
  const [isBusy, setBusy] = useState(false);

  useEffect(() => {
    if (!pushSupported()) return;
    let cancelled = false;
    void (async () => {
      try {
        if (Notification.permission === "denied") {
          if (!cancelled) setState("denied");
          return;
        }
        const registration = await swReady();
        if (!registration) return; // No SW on this surface — leave the initial state.
        const sub = await registration.pushManager.getSubscription();
        if (cancelled) return;
        if (sub) {
          setState("subscribed");
          // Reconcile: the daemon may have pruned or lost this subscription
          // (DB restore, 410 prune). Re-upserting is idempotent.
          const wire = toWireSubscription(sub);
          if (wire) await pushService.subscribe(wire).catch(() => undefined);
        } else {
          setState(
            Notification.permission === "granted" ? "unsubscribed" : "prompt",
          );
        }
      } catch {
        // SW never becomes ready (e.g. dev without SW) — leave the initial state.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const subscribe = useCallback(async () => {
    if (!pushSupported()) return;
    setBusy(true);
    try {
      const permission = await Notification.requestPermission();
      if (permission !== "granted") {
        setState(permission === "denied" ? "denied" : "prompt");
        return;
      }
      const registration = await swReady();
      if (!registration) {
        toast.error("No service worker is active — use the installed app");
        return;
      }
      const existing = await registration.pushManager.getSubscription();
      const sub =
        existing ??
        (await registration.pushManager.subscribe({
          userVisibleOnly: true,
          applicationServerKey: urlBase64ToUint8Array(
            await pushService.getVapidPublicKey(),
          ) as BufferSource,
        }));
      const wire = toWireSubscription(sub);
      if (!wire) throw new Error("browser returned an incomplete subscription");
      await pushService.subscribe(wire);
      setState("subscribed");
      toast.success("Notifications enabled");
    } catch {
      toast.error("Failed to enable notifications");
    } finally {
      setBusy(false);
    }
  }, []);

  const unsubscribe = useCallback(async () => {
    if (!pushSupported()) return;
    setBusy(true);
    try {
      const registration = await swReady();
      if (!registration) {
        toast.error("No service worker is active — use the installed app");
        return;
      }
      const sub = await registration.pushManager.getSubscription();
      if (sub) {
        await sub.unsubscribe();
        // Best-effort server cleanup — the daemon also prunes on 404/410.
        await pushService.unsubscribe(sub.endpoint).catch(() => undefined);
      }
      setState("unsubscribed");
      toast.success("Notifications disabled");
    } catch {
      toast.error("Failed to disable notifications");
    } finally {
      setBusy(false);
    }
  }, []);

  return { state, isBusy, subscribe, unsubscribe };
}
