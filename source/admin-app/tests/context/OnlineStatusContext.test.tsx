import type { ReactNode } from "react";
import { renderHook } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useOnlineStatus } = vi.hoisted(() => ({ useOnlineStatus: vi.fn() }));
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useOnlineStatus };
});

import {
  OnlineStatusProvider,
  useOnlineStatusContext,
} from "@/context/OnlineStatusContext";

function makeStatus(over: Partial<Record<string, boolean>> = {}) {
  return {
    isOnline: true,
    isDaemonReachable: true,
    showingLastKnownState: false,
    ...over,
  };
}

describe("OnlineStatusContext", () => {
  beforeEach(() => vi.clearAllMocks());

  it("throws when used outside the provider", () => {
    expect(() => renderHook(() => useOnlineStatusContext())).toThrow(
      /must be inside OnlineStatusProvider/,
    );
  });

  it("exposes the online status via context", () => {
    useOnlineStatus.mockReturnValue(makeStatus({ isDaemonReachable: false }));
    const client = new QueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>
        <OnlineStatusProvider>{children}</OnlineStatusProvider>
      </QueryClientProvider>
    );
    const { result } = renderHook(() => useOnlineStatusContext(), { wrapper });
    expect(result.current.isDaemonReachable).toBe(false);
  });

  it("invalidates daemon info queries when the daemon becomes reachable again", () => {
    // Start unreachable, then flip to reachable on rerender.
    useOnlineStatus.mockReturnValue(makeStatus({ isDaemonReachable: false }));
    const client = new QueryClient();
    const spy = vi.spyOn(client, "invalidateQueries");
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>
        <OnlineStatusProvider>{children}</OnlineStatusProvider>
      </QueryClientProvider>
    );
    const { rerender } = renderHook(() => useOnlineStatusContext(), {
      wrapper,
    });
    expect(spy).not.toHaveBeenCalled();

    useOnlineStatus.mockReturnValue(makeStatus({ isDaemonReachable: true }));
    rerender();
    expect(spy).toHaveBeenCalledWith({ queryKey: ["daemon", "info"] });
  });
});
