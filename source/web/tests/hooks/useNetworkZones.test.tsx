import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { networkZonesService, deviceService, toast } = vi.hoisted(() => ({
  networkZonesService: {
    list: vi.fn(),
    create: vi.fn(),
    update: vi.fn(),
    delete: vi.fn(),
    assignDevice: vi.fn(),
    getQuarantineNewDevices: vi.fn(),
    setQuarantineNewDevices: vi.fn(),
  },
  deviceService: { list: vi.fn() },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ networkZonesService, deviceService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import * as z from "../../src/hooks/useNetworkZones";

const w = createQueryWrapper;

describe("useNetworkZones", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists zones", async () => {
    networkZonesService.list.mockResolvedValue({ zones: [] });
    const { result } = renderHook(() => z.useNetworkZones(), { wrapper: w() });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(networkZonesService.list).toHaveBeenCalled();
  });

  it("creates, updates, and deletes a zone", async () => {
    networkZonesService.create.mockResolvedValue({ zone: {} });
    networkZonesService.update.mockResolvedValue({ zone: {} });
    networkZonesService.delete.mockResolvedValue({ deleted: true });

    const { result: c } = renderHook(() => z.useCreateNetworkZone(), {
      wrapper: w(),
    });
    await act(async () => {
      await c.current.mutateAsync({ name: "Guest" } as never);
    });
    expect(toast.success).toHaveBeenCalledWith("Zone created");

    const { result: u } = renderHook(() => z.useUpdateNetworkZone(), {
      wrapper: w(),
    });
    await act(async () => {
      await u.current.mutateAsync({ id: "z1", body: { name: "x" } });
    });
    expect(networkZonesService.update).toHaveBeenCalledWith("z1", {
      name: "x",
    });
    expect(toast.success).toHaveBeenCalledWith("Zone updated");

    const { result: d } = renderHook(() => z.useDeleteNetworkZone(), {
      wrapper: w(),
    });
    await act(async () => {
      await d.current.mutateAsync("z1");
    });
    expect(toast.success).toHaveBeenCalledWith("Zone deleted");
  });

  it("honors a custom success message on update", async () => {
    networkZonesService.update.mockResolvedValue({ zone: {} });
    const { result } = renderHook(
      () => z.useUpdateNetworkZone({ successMessage: "Home zone updated" }),
      { wrapper: w() },
    );
    await act(async () => {
      await result.current.mutateAsync({
        id: "z1",
        body: { is_default: true },
      });
    });
    expect(toast.success).toHaveBeenCalledWith("Home zone updated");
  });

  it("reassigns a device's zone", async () => {
    networkZonesService.assignDevice.mockResolvedValue({});
    const { result } = renderHook(() => z.useAssignDeviceZone(), {
      wrapper: w(),
    });
    await act(async () => {
      await result.current.mutateAsync({ deviceId: "d1", zoneId: "z2" });
    });
    expect(networkZonesService.assignDevice).toHaveBeenCalledWith("d1", "z2");
    expect(toast.success).toHaveBeenCalledWith("Zone updated");
  });

  it("surfaces an error toast when a mutation fails", async () => {
    networkZonesService.delete.mockRejectedValue(new Error("nope"));
    const { result } = renderHook(() => z.useDeleteNetworkZone(), {
      wrapper: w(),
    });
    await act(async () => {
      await result.current.mutateAsync("z1").catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Failed to delete zone");
  });

  it("surfaces error toasts for create, update, and assign failures", async () => {
    networkZonesService.create.mockRejectedValue(new Error("x"));
    networkZonesService.update.mockRejectedValue(new Error("x"));
    networkZonesService.assignDevice.mockRejectedValue(new Error("x"));

    const { result: c } = renderHook(() => z.useCreateNetworkZone(), {
      wrapper: w(),
    });
    await act(async () => {
      await c.current.mutateAsync({ name: "x" } as never).catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Failed to create zone");

    const { result: u } = renderHook(() => z.useUpdateNetworkZone(), {
      wrapper: w(),
    });
    await act(async () => {
      await u.current.mutateAsync({ id: "z1", body: {} }).catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Failed to update zone");

    const { result: a } = renderHook(() => z.useAssignDeviceZone(), {
      wrapper: w(),
    });
    await act(async () => {
      await a.current
        .mutateAsync({ deviceId: "d1", zoneId: "z1" })
        .catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Failed to reassign zone");
  });

  it("surfaces an error toast when toggling quarantine fails", async () => {
    networkZonesService.setQuarantineNewDevices.mockRejectedValue(
      new Error("x"),
    );
    const { result } = renderHook(() => z.useSetQuarantineNewDevices(), {
      wrapper: w(),
    });
    await act(async () => {
      await result.current.mutateAsync(true).catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Failed to update setting");
  });

  it("reads and toggles the quarantine setting", async () => {
    networkZonesService.getQuarantineNewDevices.mockResolvedValue({
      enabled: false,
    });
    networkZonesService.setQuarantineNewDevices.mockResolvedValue({
      enabled: true,
    });

    const { result: get } = renderHook(() => z.useQuarantineNewDevices(), {
      wrapper: w(),
    });
    await waitFor(() => expect(get.current.isSuccess).toBe(true));

    const { result: set } = renderHook(() => z.useSetQuarantineNewDevices(), {
      wrapper: w(),
    });
    await act(async () => {
      await set.current.mutateAsync(true);
    });
    expect(networkZonesService.setQuarantineNewDevices).toHaveBeenCalledWith({
      enabled: true,
    });
    expect(toast.success).toHaveBeenCalledWith(
      "New-device notifications enabled",
    );

    await act(async () => {
      await set.current.mutateAsync(false);
    });
    expect(toast.success).toHaveBeenCalledWith(
      "New-device notifications disabled",
    );
  });
});

describe("usePendingDevices", () => {
  beforeEach(() => vi.clearAllMocks());

  it("derives the inbox: default-for-new members, most-recent-first, + home", async () => {
    networkZonesService.list.mockResolvedValue({
      zones: [
        { id: "home", is_default: true, is_default_for_new: false },
        { id: "guest", is_default: false, is_default_for_new: true },
      ],
    });
    deviceService.list.mockResolvedValue({
      devices: [
        { id: "old", zone_id: "guest", first_seen: "2026-01-01T00:00:00Z" },
        { id: "new", zone_id: "guest", first_seen: "2026-06-01T00:00:00Z" },
        { id: "home-dev", zone_id: "home", first_seen: "2026-06-02T00:00:00Z" },
      ],
    });
    const { result } = renderHook(() => z.usePendingDevices(), {
      wrapper: w(),
    });
    await waitFor(() => expect(result.current.pending.length).toBe(2));
    // Most-recent-first, home-zone device excluded.
    expect(result.current.pending.map((d) => d.id)).toEqual(["new", "old"]);
    expect(result.current.defaultForNew?.id).toBe("guest");
    expect(result.current.homeZone?.id).toBe("home");
  });

  it("returns an empty inbox when no zone is flagged default-for-new", async () => {
    networkZonesService.list.mockResolvedValue({
      zones: [{ id: "home", is_default: true, is_default_for_new: false }],
    });
    deviceService.list.mockResolvedValue({
      devices: [
        { id: "d", zone_id: "home", first_seen: "2026-01-01T00:00:00Z" },
      ],
    });
    const { result } = renderHook(() => z.usePendingDevices(), {
      wrapper: w(),
    });
    await waitFor(() => expect(result.current.homeZone?.id).toBe("home"));
    expect(result.current.pending).toEqual([]);
    expect(result.current.defaultForNew).toBeUndefined();
  });
});
