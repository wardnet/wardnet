import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { pushService, toast } = vi.hoisted(() => ({
  pushService: {
    getVapidPublicKey: vi.fn(),
    subscribe: vi.fn(),
    unsubscribe: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ pushService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import { usePushNotifications } from "../../src/hooks/usePushNotifications";

/** A fake browser PushSubscription. */
function makeSubscription(endpoint = "https://push.example/sub-1") {
  return {
    endpoint,
    toJSON: () => ({ endpoint, keys: { p256dh: "pk", auth: "au" } }),
    unsubscribe: vi.fn().mockResolvedValue(true),
  };
}

const notification = {
  permission: "default" as string,
  requestPermission: vi.fn(),
};
const pushManager = { getSubscription: vi.fn(), subscribe: vi.fn() };
const registration = { pushManager };

/** Install the Web Push browser APIs jsdom lacks. */
function installPushApis() {
  Object.defineProperty(window, "Notification", {
    value: notification,
    configurable: true,
  });
  Object.defineProperty(window, "PushManager", {
    value: class {},
    configurable: true,
  });
  Object.defineProperty(navigator, "serviceWorker", {
    value: { ready: Promise.resolve(registration) },
    configurable: true,
  });
}

function uninstallPushApis() {
  // @ts-expect-error test cleanup of injected globals
  delete window.Notification;
  // @ts-expect-error test cleanup of injected globals
  delete window.PushManager;
  // @ts-expect-error test cleanup of injected globals
  delete navigator.serviceWorker;
}

describe("usePushNotifications", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    notification.permission = "default";
    pushManager.getSubscription.mockResolvedValue(null);
  });
  afterEach(uninstallPushApis);

  it("reports unsupported when the browser lacks the push APIs", () => {
    const { result } = renderHook(() => usePushNotifications());
    expect(result.current.state).toBe("unsupported");
  });

  it("reports denied when notification permission is blocked", async () => {
    installPushApis();
    notification.permission = "denied";
    const { result } = renderHook(() => usePushNotifications());
    await waitFor(() => expect(result.current.state).toBe("denied"));
  });

  it("re-registers an existing browser subscription on mount (reconciliation)", async () => {
    installPushApis();
    notification.permission = "granted";
    pushManager.getSubscription.mockResolvedValue(makeSubscription());
    pushService.subscribe.mockResolvedValue(undefined);

    const { result } = renderHook(() => usePushNotifications());
    await waitFor(() => expect(result.current.state).toBe("subscribed"));
    expect(pushService.subscribe).toHaveBeenCalledWith({
      endpoint: "https://push.example/sub-1",
      keys: { p256dh: "pk", auth: "au" },
    });
  });

  it("reports unsubscribed when permission is granted but no subscription exists", async () => {
    installPushApis();
    notification.permission = "granted";
    const { result } = renderHook(() => usePushNotifications());
    await waitFor(() => expect(result.current.state).toBe("unsubscribed"));
    expect(pushService.subscribe).not.toHaveBeenCalled();
  });

  it("subscribe() walks permission → VAPID key → pushManager → daemon", async () => {
    installPushApis();
    notification.requestPermission.mockImplementation(async () => {
      notification.permission = "granted";
      return "granted";
    });
    // "AAAA" -> base64url of three zero bytes.
    pushService.getVapidPublicKey.mockResolvedValue("AAAA");
    pushService.subscribe.mockResolvedValue(undefined);
    const sub = makeSubscription("https://push.example/new");
    pushManager.subscribe.mockResolvedValue(sub);

    const { result } = renderHook(() => usePushNotifications());
    await waitFor(() => expect(result.current.state).toBe("prompt"));

    await act(async () => {
      await result.current.subscribe();
    });

    expect(pushManager.subscribe).toHaveBeenCalledWith(
      expect.objectContaining({ userVisibleOnly: true }),
    );
    const args = pushManager.subscribe.mock.calls[0][0];
    expect(new Uint8Array(args.applicationServerKey)).toEqual(
      new Uint8Array([0, 0, 0]),
    );
    expect(pushService.subscribe).toHaveBeenCalledWith({
      endpoint: "https://push.example/new",
      keys: { p256dh: "pk", auth: "au" },
    });
    expect(result.current.state).toBe("subscribed");
    expect(toast.success).toHaveBeenCalledWith("Notifications enabled");
  });

  it("subscribe() surfaces a denied prompt without subscribing", async () => {
    installPushApis();
    notification.requestPermission.mockImplementation(async () => {
      notification.permission = "denied";
      return "denied";
    });

    const { result } = renderHook(() => usePushNotifications());
    await waitFor(() => expect(result.current.state).toBe("prompt"));
    await act(async () => {
      await result.current.subscribe();
    });

    expect(result.current.state).toBe("denied");
    expect(pushManager.subscribe).not.toHaveBeenCalled();
    expect(pushService.subscribe).not.toHaveBeenCalled();
  });

  it("subscribe() toasts an error when the daemon rejects the subscription", async () => {
    installPushApis();
    notification.requestPermission.mockImplementation(async () => {
      notification.permission = "granted";
      return "granted";
    });
    pushService.getVapidPublicKey.mockResolvedValue("AAAA");
    pushManager.subscribe.mockResolvedValue(makeSubscription());
    pushService.subscribe.mockRejectedValue(new Error("boom"));

    const { result } = renderHook(() => usePushNotifications());
    await waitFor(() => expect(result.current.state).toBe("prompt"));
    await act(async () => {
      await result.current.subscribe();
    });

    expect(toast.error).toHaveBeenCalledWith("Failed to enable notifications");
  });

  it("subscribe() fails fast with a toast when no service worker ever activates", async () => {
    vi.useFakeTimers();
    try {
      installPushApis();
      // A surface with the push APIs but no registered SW (e.g. vite dev):
      // `.ready` never settles.
      Object.defineProperty(navigator, "serviceWorker", {
        value: { ready: new Promise(() => undefined) },
        configurable: true,
      });
      notification.requestPermission.mockImplementation(async () => {
        notification.permission = "granted";
        return "granted";
      });

      const { result } = renderHook(() => usePushNotifications());
      const pending = act(async () => {
        const p = result.current.subscribe();
        await vi.advanceTimersByTimeAsync(5_000);
        await p;
      });
      await pending;

      expect(toast.error).toHaveBeenCalledWith(
        "No service worker is active — use the installed app",
      );
      expect(pushManager.subscribe).not.toHaveBeenCalled();
      expect(result.current.isBusy).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("unsubscribe() removes the browser subscription and tells the daemon", async () => {
    installPushApis();
    notification.permission = "granted";
    const sub = makeSubscription();
    pushManager.getSubscription.mockResolvedValue(sub);
    pushService.subscribe.mockResolvedValue(undefined);
    pushService.unsubscribe.mockResolvedValue(undefined);

    const { result } = renderHook(() => usePushNotifications());
    await waitFor(() => expect(result.current.state).toBe("subscribed"));

    await act(async () => {
      await result.current.unsubscribe();
    });

    expect(sub.unsubscribe).toHaveBeenCalled();
    expect(pushService.unsubscribe).toHaveBeenCalledWith(
      "https://push.example/sub-1",
    );
    expect(result.current.state).toBe("unsubscribed");
  });
});
