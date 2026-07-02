import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, renderHook, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import {
  OnlineStatusProvider,
  useOnlineStatusContext,
} from "../../src/context/OnlineStatusContext";

const { useOnlineStatus } = vi.hoisted(() => ({ useOnlineStatus: vi.fn() }));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useOnlineStatus };
});

let queryClient: QueryClient;
let invalidateSpy: ReturnType<typeof vi.spyOn>;

function wrapper({ children }: { children: ReactNode }) {
  return (
    <QueryClientProvider client={queryClient}>
      <OnlineStatusProvider>{children}</OnlineStatusProvider>
    </QueryClientProvider>
  );
}

beforeEach(() => {
  queryClient = new QueryClient();
  invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
  useOnlineStatus.mockReturnValue({
    isOnline: true,
    isDaemonReachable: true,
    showingLastKnownState: false,
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("OnlineStatusContext", () => {
  it("exposes the online status via the context hook", () => {
    const { result } = renderHook(() => useOnlineStatusContext(), { wrapper });
    expect(result.current.isDaemonReachable).toBe(true);
  });

  it("throws when used outside the provider", () => {
    // Silence the expected error log from React.
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => renderHook(() => useOnlineStatusContext())).toThrow(
      /must be inside OnlineStatusProvider/,
    );
    spy.mockRestore();
  });

  it("invalidates the device query when the daemon becomes reachable again", () => {
    useOnlineStatus.mockReturnValue({
      isOnline: true,
      isDaemonReachable: false,
      showingLastKnownState: true,
    });
    const { rerender } = render(<div>child</div>, { wrapper });
    expect(screen.getByText("child")).toBeInTheDocument();
    invalidateSpy.mockClear();

    // Daemon comes back — the effect should invalidate ["devices","me"].
    useOnlineStatus.mockReturnValue({
      isOnline: true,
      isDaemonReachable: true,
      showingLastKnownState: false,
    });
    rerender(<div>child</div>);

    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ["devices", "me"],
    });
  });

  it("does not invalidate while the daemon stays reachable", () => {
    render(<div>child</div>, { wrapper });
    expect(invalidateSpy).not.toHaveBeenCalled();
  });
});
