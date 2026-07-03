import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { networkZonesService, toast } = vi.hoisted(() => ({
  networkZonesService: {
    list: vi.fn(),
    create: vi.fn(),
    update: vi.fn(),
    delete: vi.fn(),
    assignDevice: vi.fn(),
    getQuarantineNewDevices: vi.fn(),
    setQuarantineNewDevices: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ networkZonesService }));
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
