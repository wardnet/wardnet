import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { providerService, toast } = vi.hoisted(() => ({
  providerService: {
    list: vi.fn(),
    listCountries: vi.fn(),
    validateCredentials: vi.fn(),
    listServers: vi.fn(),
    setupTunnel: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ providerService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import {
  useProviders,
  useProviderCountries,
  useValidateCredentials,
  useProviderServers,
  useProviderSetup,
} from "../../src/hooks/useProviders";

describe("useProviders hooks", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists providers", async () => {
    providerService.list.mockResolvedValue({ providers: [] });
    const { result } = renderHook(() => useProviders(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(providerService.list).toHaveBeenCalledOnce();
  });

  it("does not fetch countries without a provider id", () => {
    const { result } = renderHook(() => useProviderCountries(""), {
      wrapper: createQueryWrapper(),
    });
    expect(result.current.fetchStatus).toBe("idle");
    expect(providerService.listCountries).not.toHaveBeenCalled();
  });

  it("fetches countries for a provider id", async () => {
    providerService.listCountries.mockResolvedValue({ countries: [] });
    const { result } = renderHook(() => useProviderCountries("nordvpn"), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(providerService.listCountries).toHaveBeenCalledWith("nordvpn");
  });

  it("validates credentials", async () => {
    providerService.validateCredentials.mockResolvedValue({ valid: true });
    const { result } = renderHook(() => useValidateCredentials(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({
        providerId: "p",
        body: { token: "t" } as never,
      });
    });
    expect(providerService.validateCredentials).toHaveBeenCalledWith("p", {
      token: "t",
    });
  });

  it("lists servers via mutation", async () => {
    providerService.listServers.mockResolvedValue({ servers: [] });
    const { result } = renderHook(() => useProviderServers(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ providerId: "p", body: {} as never });
    });
    expect(providerService.listServers).toHaveBeenCalledWith("p", {});
  });

  it("sets up a tunnel and toasts success", async () => {
    providerService.setupTunnel.mockResolvedValue({ message: "done" });
    const { result } = renderHook(() => useProviderSetup(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ providerId: "p", body: {} as never });
    });
    expect(toast.success).toHaveBeenCalledWith("done");
  });

  it("toasts an error when tunnel setup fails", async () => {
    providerService.setupTunnel.mockRejectedValue(new Error("nope"));
    const { result } = renderHook(() => useProviderSetup(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current
        .mutateAsync({ providerId: "p", body: {} as never })
        .catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Failed to set up tunnel");
  });
});
