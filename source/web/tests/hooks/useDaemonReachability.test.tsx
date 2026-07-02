import { renderHook, act } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WardnetApiError } from "@wardnet/js";

const { systemService } = vi.hoisted(() => ({
  systemService: { getStatus: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ systemService }));

import { useDaemonReachability } from "../../src/hooks/useDaemonReachability";

function stubFetch(fn: () => Promise<{ ok: boolean }>) {
  vi.stubGlobal("fetch", vi.fn(fn as never));
}

describe("useDaemonReachability", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("starts idle and exposes isOpen once scheduled", () => {
    const { result } = renderHook(() => useDaemonReachability());
    expect(result.current.phase).toBe("idle");
    expect(result.current.isOpen).toBe(false);
    act(() => result.current.markScheduled());
    expect(result.current.phase).toBe("scheduled");
    expect(result.current.isOpen).toBe(true);
  });

  it("fail() moves to failed with a message", () => {
    const { result } = renderHook(() => useDaemonReachability());
    act(() => result.current.fail("nope"));
    expect(result.current.phase).toBe("failed");
    expect(result.current.errorMessage).toBe("nope");
  });

  it("reset() returns to idle", () => {
    const { result } = renderHook(() => useDaemonReachability());
    act(() => result.current.markScheduled());
    act(() => result.current.reset());
    expect(result.current.phase).toBe("idle");
  });

  it("restart: recovers to ready after a bounce with a valid session", async () => {
    let calls = 0;
    stubFetch(() => {
      calls += 1;
      return Promise.resolve({ ok: calls > 1 });
    });
    systemService.getStatus.mockResolvedValue({});
    const { result } = renderHook(() => useDaemonReachability());
    act(() => result.current.start({ kind: "restart" }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(result.current.phase).toBe("ready");
  });

  it("restart: reports ready_signed_out when the session probe 401s", async () => {
    let calls = 0;
    stubFetch(() => {
      calls += 1;
      return Promise.resolve({ ok: calls > 1 });
    });
    systemService.getStatus.mockRejectedValue(
      new WardnetApiError(401, "Unauthorized", { error: "unauthorized" }),
    );
    const { result } = renderHook(() => useDaemonReachability());
    act(() => result.current.start({ kind: "restart" }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(result.current.phase).toBe("ready_signed_out");
  });

  it("restart: treats a non-401 session error as ready", async () => {
    let calls = 0;
    stubFetch(() => {
      calls += 1;
      return Promise.resolve({ ok: calls > 1 });
    });
    systemService.getStatus.mockRejectedValue(new Error("5xx"));
    const { result } = renderHook(() => useDaemonReachability());
    act(() => result.current.start({ kind: "restart" }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(result.current.phase).toBe("ready");
  });

  it("shutdown: resolves to off after enough consecutive down probes", async () => {
    stubFetch(() => Promise.reject(new Error("down")));
    const { result } = renderHook(() => useDaemonReachability());
    act(() => result.current.start({ kind: "shutdown" }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(result.current.phase).toBe("off");
  });

  it("reports did_not_fire when the daemon stays reachable past the grace window", async () => {
    stubFetch(() => Promise.resolve({ ok: true }));
    const { result } = renderHook(() => useDaemonReachability());
    act(() => result.current.start({ kind: "restart" }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(9000);
    });
    expect(result.current.phase).toBe("did_not_fire");
  });

  it("reports timeout when no terminal state is reached in time", async () => {
    stubFetch(() => Promise.resolve({ ok: false }));
    const { result } = renderHook(() => useDaemonReachability());
    act(() => result.current.start({ kind: "restart" }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(46000);
    });
    expect(result.current.phase).toBe("timeout");
  });
});
