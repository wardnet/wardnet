import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  isInScope,
  registerPushHandlers,
  resolveClickUrl,
} from "../../src/lib/swPush";

const SCOPE = "https://pi.local/admin-app/";

type Handler = (event: unknown) => void;

/** Minimal fake ServiceWorkerGlobalScope capturing registered handlers. */
function makeFakeSw() {
  const handlers: Record<string, Handler> = {};
  const showNotification = vi.fn().mockResolvedValue(undefined);
  const openWindow = vi.fn().mockResolvedValue(null);
  const matchAll = vi.fn().mockResolvedValue([]);
  const sw = {
    addEventListener: (type: string, handler: Handler) => {
      handlers[type] = handler;
    },
    registration: { scope: SCOPE, showNotification },
    clients: { matchAll, openWindow },
  };
  return { sw, handlers, showNotification, openWindow, matchAll };
}

function pushEvent(payload: unknown) {
  const waited: Promise<unknown>[] = [];
  return {
    event: {
      data: { json: () => payload },
      waitUntil: (p: Promise<unknown>) => waited.push(p),
    },
    settle: () => Promise.all(waited),
  };
}

function clickEvent(data: unknown) {
  const waited: Promise<unknown>[] = [];
  return {
    event: {
      notification: { close: vi.fn(), data },
      waitUntil: (p: Promise<unknown>) => waited.push(p),
    },
    settle: () => Promise.all(waited),
  };
}

describe("resolveClickUrl", () => {
  it("resolves app-relative URLs against the SW scope", () => {
    expect(resolveClickUrl("/devices", SCOPE)).toBe(
      "https://pi.local/admin-app/devices",
    );
  });

  it("falls back to the scope root when the payload has no URL", () => {
    expect(resolveClickUrl(undefined, SCOPE)).toBe(SCOPE);
  });
});

describe("isInScope", () => {
  it("matches windows under the scope", () => {
    expect(isInScope(`${SCOPE}devices`, SCOPE)).toBe(true);
  });

  it("matches a window at the slashless scope root", () => {
    // The daemon serves the app shell at /admin-app (no redirect), so the
    // document URL can lack the trailing slash the scope always has.
    expect(isInScope("https://pi.local/admin-app", SCOPE)).toBe(true);
    expect(isInScope("https://pi.local/admin-app?x=1", SCOPE)).toBe(true);
  });

  it("rejects windows outside the scope", () => {
    expect(isInScope("https://pi.local/", SCOPE)).toBe(false);
    expect(isInScope("https://pi.local/admin-apple", SCOPE)).toBe(false);
  });
});

describe("registerPushHandlers", () => {
  let fake: ReturnType<typeof makeFakeSw>;

  beforeEach(() => {
    fake = makeFakeSw();
    registerPushHandlers(fake.sw as unknown as ServiceWorkerGlobalScope, {
      icon: "icons/admin-192.png",
      badge: "icons/badge-96.png",
    });
  });

  it("shows a notification for a daemon payload, tagged by kind", async () => {
    const { event, settle } = pushEvent({
      title: "New device",
      body: "New device Phone joined, in Guest. Approve in the app.",
      data: {
        kind: "new_device_quarantined",
        url: "/devices",
        subject_id: "d1",
      },
    });
    fake.handlers.push(event);
    await settle();

    expect(fake.showNotification).toHaveBeenCalledWith("New device", {
      body: "New device Phone joined, in Guest. Approve in the app.",
      // Tagged per subject so two different devices/tunnels of the same kind
      // do not collapse into one notification.
      tag: "new_device_quarantined:d1",
      icon: "icons/admin-192.png",
      badge: "icons/badge-96.png",
      data: {
        kind: "new_device_quarantined",
        url: "/devices",
        subject_id: "d1",
      },
    });
  });

  it("ignores payloads without a title", async () => {
    const { event, settle } = pushEvent({ body: "no title" });
    fake.handlers.push(event);
    await settle();
    expect(fake.showNotification).not.toHaveBeenCalled();
  });

  it("opens a new window on the deep link when no app window exists", async () => {
    const { event, settle } = clickEvent({
      kind: "new_device_quarantined",
      url: "/devices",
    });
    fake.handlers.notificationclick(event);
    await settle();

    expect(event.notification.close).toHaveBeenCalled();
    expect(fake.openWindow).toHaveBeenCalledWith(
      "https://pi.local/admin-app/devices",
    );
  });

  it("focuses and navigates an existing app window instead of opening one", async () => {
    const client = {
      url: `${SCOPE}system`,
      focus: vi.fn().mockResolvedValue(undefined),
      navigate: vi.fn().mockResolvedValue(undefined),
    };
    fake.matchAll.mockResolvedValue([client]);

    const { event, settle } = clickEvent({
      kind: "tunnel_offline",
      url: "/tunnels",
    });
    fake.handlers.notificationclick(event);
    await settle();

    expect(client.focus).toHaveBeenCalled();
    expect(client.navigate).toHaveBeenCalledWith(
      "https://pi.local/admin-app/tunnels",
    );
    expect(fake.openWindow).not.toHaveBeenCalled();
  });

  it("falls back to openWindow when navigate() rejects on an uncontrolled window", async () => {
    // WindowClient.navigate() rejects for clients this SW does not control
    // (a tab opened before the first SW activation). The deep link must still
    // land via openWindow.
    const client = {
      url: `${SCOPE}system`,
      focus: vi.fn().mockResolvedValue(undefined),
      navigate: vi.fn().mockRejectedValue(new TypeError("uncontrolled")),
    };
    fake.matchAll.mockResolvedValue([client]);

    const { event, settle } = clickEvent({
      kind: "new_device_quarantined",
      url: "/devices",
    });
    fake.handlers.notificationclick(event);
    await settle();

    expect(client.focus).toHaveBeenCalled();
    expect(fake.openWindow).toHaveBeenCalledWith(
      "https://pi.local/admin-app/devices",
    );
  });
});
