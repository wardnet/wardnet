import { renderHook, act, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useOnlineStatus } from "../../src/hooks/useOnlineStatus";

describe("useOnlineStatus", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: true } as Response));
    Object.defineProperty(navigator, "onLine", {
      configurable: true,
      value: true,
    });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("reports online + reachable after the first successful probe", async () => {
    const { result } = renderHook(() =>
      useOnlineStatus({ daemonProbeIntervalMs: 10_000 }),
    );
    await waitFor(() => expect(result.current.isDaemonReachable).toBe(true));
    expect(result.current.isOnline).toBe(true);
    expect(result.current.showingLastKnownState).toBe(false);
  });

  it("flips isOnline on window offline/online events", async () => {
    const { result } = renderHook(() => useOnlineStatus());
    act(() => {
      window.dispatchEvent(new Event("offline"));
    });
    await waitFor(() => expect(result.current.isOnline).toBe(false));
    expect(result.current.showingLastKnownState).toBe(true);
    act(() => {
      window.dispatchEvent(new Event("online"));
    });
    await waitFor(() => expect(result.current.isOnline).toBe(true));
  });

  it("marks the daemon unreachable when the probe rejects", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("down")));
    const { result } = renderHook(() => useOnlineStatus());
    await waitFor(() => expect(result.current.isDaemonReachable).toBe(false));
  });

  it("marks the daemon unreachable on a non-2xx probe", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({ ok: false } as Response),
    );
    const { result } = renderHook(() => useOnlineStatus());
    await waitFor(() => expect(result.current.isDaemonReachable).toBe(false));
  });
});
