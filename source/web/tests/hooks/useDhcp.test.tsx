import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { dhcpService, toast } = vi.hoisted(() => ({
  dhcpService: {
    status: vi.fn(),
    getConfig: vi.fn(),
    listLeases: vi.fn(),
    listReservations: vi.fn(),
    toggle: vi.fn(),
    previewConfig: vi.fn(),
    updateConfig: vi.fn(),
    createReservation: vi.fn(),
    deleteReservation: vi.fn(),
    revokeLease: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ dhcpService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import {
  useDhcpStatus,
  useDhcpConfig,
  useDhcpLeases,
  useDhcpReservations,
  useToggleDhcp,
  usePreviewDhcpConfig,
  useUpdateDhcpConfig,
  useCreateReservation,
  useDeleteReservation,
  useRevokeLease,
} from "../../src/hooks/useDhcp";

const wrapper = createQueryWrapper;

describe("useDhcp queries", () => {
  beforeEach(() => vi.clearAllMocks());

  it.each([
    ["status", useDhcpStatus, "status"],
    ["config", useDhcpConfig, "getConfig"],
    ["leases", useDhcpLeases, "listLeases"],
    ["reservations", useDhcpReservations, "listReservations"],
  ] as const)("%s query calls its service method", async (_n, hook, method) => {
    (dhcpService[method] as ReturnType<typeof vi.fn>).mockResolvedValue({});
    const { result } = renderHook(() => hook(), { wrapper: wrapper() });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(dhcpService[method]).toHaveBeenCalled();
  });
});

describe("useDhcp mutations", () => {
  beforeEach(() => vi.clearAllMocks());

  it("toggles the DHCP server", async () => {
    dhcpService.toggle.mockResolvedValue({ config: { enabled: true } });
    const { result } = renderHook(() => useToggleDhcp(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync(true);
    });
    expect(toast.success).toHaveBeenCalledWith("DHCP server enabled");
  });

  it("previews config without a toast", async () => {
    dhcpService.previewConfig.mockResolvedValue({ warnings: [] });
    const { result } = renderHook(() => usePreviewDhcpConfig(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ pool_start: "10.0.0.10" } as never);
    });
    expect(dhcpService.previewConfig).toHaveBeenCalled();
  });

  it("updates config", async () => {
    dhcpService.updateConfig.mockResolvedValue({});
    const { result } = renderHook(() => useUpdateDhcpConfig(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ pool_start: "10.0.0.10" } as never);
    });
    expect(toast.success).toHaveBeenCalledWith("DHCP configuration updated");
  });

  it("creates a reservation using the server message", async () => {
    dhcpService.createReservation.mockResolvedValue({ message: "Reserved" });
    const { result } = renderHook(() => useCreateReservation(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ mac: "aa", ip: "10.0.0.5" } as never);
    });
    expect(toast.success).toHaveBeenCalledWith("Reserved");
  });

  it("deletes a reservation with a default message", async () => {
    dhcpService.deleteReservation.mockResolvedValue({ message: "" });
    const { result } = renderHook(() => useDeleteReservation(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("res-1");
    });
    expect(toast.success).toHaveBeenCalledWith("Reservation deleted");
  });

  it("revokes a lease and toasts an error on failure", async () => {
    dhcpService.revokeLease.mockRejectedValue(new Error("x"));
    const { result } = renderHook(() => useRevokeLease(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("lease-1").catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Failed to revoke lease");
  });
});
