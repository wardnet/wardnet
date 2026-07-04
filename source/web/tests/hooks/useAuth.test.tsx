import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { authService, systemService } = vi.hoisted(() => ({
  authService: { me: vi.fn(), login: vi.fn(), refresh: vi.fn() },
  systemService: { getStatus: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ authService, systemService }));

import { useAuth, useMe } from "../../src/hooks/useAuth";

describe("useAuth", () => {
  beforeEach(() => vi.clearAllMocks());

  it("exposes the auth store state and actions", () => {
    const { result } = renderHook(() => useAuth());
    expect(typeof result.current.login).toBe("function");
    expect(typeof result.current.logout).toBe("function");
    expect(typeof result.current.checkAuth).toBe("function");
    expect("isAdmin" in result.current).toBe(true);
  });

  it("useMe fetches the authenticated admin identity", async () => {
    authService.me.mockResolvedValue({ username: "operator" });
    const { result } = renderHook(() => useMe(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ username: "operator" });
  });
});
