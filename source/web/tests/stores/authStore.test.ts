import { beforeEach, describe, expect, it, vi } from "vitest";
import { WardnetApiError } from "@wardnet/js";

const { authService, systemService } = vi.hoisted(() => ({
  authService: { login: vi.fn(), logout: vi.fn(), refresh: vi.fn() },
  systemService: { getStatus: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ authService, systemService }));

import { useAuthStore } from "../../src/stores/authStore";

function reset() {
  useAuthStore.setState({ isAdmin: false, isChecking: true });
}

describe("authStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    reset();
  });

  it("login sets isAdmin and forwards credentials", async () => {
    authService.login.mockResolvedValue(undefined);
    await useAuthStore.getState().login("admin", "pw", true);
    expect(authService.login).toHaveBeenCalledWith({
      username: "admin",
      password: "pw",
      rememberMe: true,
    });
    expect(useAuthStore.getState().isAdmin).toBe(true);
  });

  it("login defaults rememberMe to false", async () => {
    authService.login.mockResolvedValue(undefined);
    await useAuthStore.getState().login("admin", "pw");
    expect(authService.login).toHaveBeenCalledWith({
      username: "admin",
      password: "pw",
      rememberMe: false,
    });
  });

  it("logout revokes the server session and clears isAdmin", async () => {
    authService.logout.mockResolvedValue(undefined);
    useAuthStore.setState({ isAdmin: true });
    await useAuthStore.getState().logout();
    expect(authService.logout).toHaveBeenCalledOnce();
    expect(useAuthStore.getState().isAdmin).toBe(false);
  });

  it("logout clears isAdmin even when the network call fails", async () => {
    authService.logout.mockRejectedValue(new Error("network down"));
    useAuthStore.setState({ isAdmin: true });
    await expect(useAuthStore.getState().logout()).resolves.toBeUndefined();
    expect(useAuthStore.getState().isAdmin).toBe(false);
  });

  it("checkAuth marks admin when the status probe succeeds", async () => {
    systemService.getStatus.mockResolvedValue({});
    await useAuthStore.getState().checkAuth();
    expect(useAuthStore.getState()).toMatchObject({
      isAdmin: true,
      isChecking: false,
    });
  });

  it("checkAuth clears admin on a 401", async () => {
    systemService.getStatus.mockRejectedValue(
      new WardnetApiError(401, "Unauthorized", { error: "unauthorized" }),
    );
    await useAuthStore.getState().checkAuth();
    expect(useAuthStore.getState()).toMatchObject({
      isAdmin: false,
      isChecking: false,
    });
  });

  it("checkAuth clears admin on a network error", async () => {
    systemService.getStatus.mockRejectedValue(new Error("network down"));
    await useAuthStore.getState().checkAuth();
    expect(useAuthStore.getState()).toMatchObject({
      isAdmin: false,
      isChecking: false,
    });
  });

  it("refresh swallows errors", async () => {
    authService.refresh.mockRejectedValue(new Error("boom"));
    await expect(useAuthStore.getState().refresh()).resolves.toBeUndefined();
    expect(authService.refresh).toHaveBeenCalledOnce();
  });
});
