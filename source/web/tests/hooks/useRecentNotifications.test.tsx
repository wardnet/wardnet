import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { pushService, toast, authState } = vi.hoisted(() => ({
  pushService: {
    listNotifications: vi.fn(),
    clearNotifications: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
  authState: { isAdmin: true },
}));
vi.mock("../../src/lib/sdk", () => ({ pushService }));
vi.mock("../../src/hooks/useAuth", () => ({ useAuth: () => authState }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import {
  useClearNotifications,
  useRecentNotifications,
} from "../../src/hooks/useRecentNotifications";

const w = createQueryWrapper;

describe("useRecentNotifications", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    authState.isAdmin = true;
  });

  it("polls the feed when an admin session exists", async () => {
    pushService.listNotifications.mockResolvedValue([
      {
        id: "n1",
        kind: "tunnel_offline",
        title: "Tunnel offline",
        body: "Sweden #12 went offline.",
        url: "/tunnels",
        created_at: "2026-07-03T00:00:00Z",
      },
    ]);
    const { result } = renderHook(() => useRecentNotifications(), {
      wrapper: w(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(pushService.listNotifications).toHaveBeenCalledWith(50);
    expect(result.current.data?.[0].kind).toBe("tunnel_offline");
  });

  it("does not fetch without an admin session", () => {
    authState.isAdmin = false;
    renderHook(() => useRecentNotifications(), { wrapper: w() });
    expect(pushService.listNotifications).not.toHaveBeenCalled();
  });

  it("clears the feed and invalidates the query", async () => {
    pushService.clearNotifications.mockResolvedValue(undefined);
    const { result } = renderHook(() => useClearNotifications(), {
      wrapper: w(),
    });
    await act(async () => {
      await result.current.mutateAsync();
    });
    expect(pushService.clearNotifications).toHaveBeenCalled();
    expect(toast.success).toHaveBeenCalledWith("Notifications cleared");
  });
});
