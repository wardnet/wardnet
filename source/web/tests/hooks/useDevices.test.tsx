import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

// Mock the SDK singletons the hooks call. `vi.hoisted` runs the spy setup
// before the hoisted `vi.mock` factory, so the factory can safely close over
// them (a plain outer const would be uninitialised at hoist time).
const { deviceService } = vi.hoisted(() => ({
  deviceService: {
    list: vi.fn(),
    getById: vi.fn(),
    getMe: vi.fn(),
    setMyRule: vi.fn(),
    setMyCaptureEnabled: vi.fn(),
    update: vi.fn(),
    release: vi.fn(),
    getDnsCaptureSettings: vi.fn(),
    updateDnsCaptureSettings: vi.fn(),
    identify: vi.fn(),
  },
}));
vi.mock("../../src/lib/sdk", () => ({ deviceService }));

// Silence (and observe) the toast side-effects the mutation hooks fire.
const { toast } = vi.hoisted(() => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import { act } from "react";
import {
  useDevices,
  useDevice,
  useMyDevice,
  useSetMyRule,
  useSetMyCaptureEnabled,
  useUpdateDevice,
  useIdentifyDevice,
  useReleaseDevice,
  useDnsCaptureSettings,
  useUpdateDnsCaptureSettings,
} from "../../src/hooks/useDevices";

describe("useDevices", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("fetches the device list via deviceService.list", async () => {
    const devices = [{ id: "d1" }];
    deviceService.list.mockResolvedValue({ devices });

    const { result } = renderHook(() => useDevices(), {
      wrapper: createQueryWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(deviceService.list).toHaveBeenCalledOnce();
    expect(result.current.data).toEqual({ devices });
  });

  it("fetches a device by id and skips when id is empty", async () => {
    deviceService.getById.mockResolvedValue({ id: "d1" });
    const { result } = renderHook(() => useDevice("d1"), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(deviceService.getById).toHaveBeenCalledWith("d1");

    const { result: empty } = renderHook(() => useDevice(""), {
      wrapper: createQueryWrapper(),
    });
    expect(empty.current.fetchStatus).toBe("idle");
  });

  it("fetches the caller's own device", async () => {
    deviceService.getMe.mockResolvedValue({ id: "me" });
    const { result } = renderHook(() => useMyDevice(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(deviceService.getMe).toHaveBeenCalledOnce();
  });

  it("sets the caller's routing rule", async () => {
    deviceService.setMyRule.mockResolvedValue({});
    const { result } = renderHook(() => useSetMyRule(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ type: "direct" });
    });
    expect(deviceService.setMyRule).toHaveBeenCalledWith({ type: "direct" });
    expect(toast.success).toHaveBeenCalledWith("Routing updated");
  });

  it("toggles the caller's DNS capture with contextual copy", async () => {
    deviceService.setMyCaptureEnabled.mockResolvedValue({});
    const { result } = renderHook(() => useSetMyCaptureEnabled(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync(true);
    });
    expect(toast.success).toHaveBeenCalledWith("DNS capture enabled");
    await act(async () => {
      await result.current.mutateAsync(false);
    });
    expect(toast.success).toHaveBeenCalledWith("DNS capture disabled");
  });

  it("updates a device with a custom success message", async () => {
    deviceService.update.mockResolvedValue({});
    const { result } = renderHook(
      () => useUpdateDevice({ successMessage: "Saved!" }),
      { wrapper: createQueryWrapper() },
    );
    await act(async () => {
      await result.current.mutateAsync({ id: "d1", body: { name: "TV" } });
    });
    expect(deviceService.update).toHaveBeenCalledWith("d1", { name: "TV" });
    expect(toast.success).toHaveBeenCalledWith("Saved!");
  });

  it("reports how many ports answered an identification probe", async () => {
    deviceService.identify.mockResolvedValue({
      ports_probed: [4003, 6053, 6668, 8009],
      answering_ports: [6668],
      device: {},
    });

    const { result } = renderHook(() => useIdentifyDevice(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("d1");
    });

    expect(deviceService.identify).toHaveBeenCalledWith("d1");
    expect(toast.success).toHaveBeenCalledWith("Identified: 1 port answered");
  });

  it("pluralises the answering-port count", async () => {
    deviceService.identify.mockResolvedValue({
      ports_probed: [4003, 6053, 6668, 8009],
      answering_ports: [6053, 6668],
      device: {},
    });

    const { result } = renderHook(() => useIdentifyDevice(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("d1");
    });

    expect(toast.success).toHaveBeenCalledWith("Identified: 2 ports answered");
  });

  it("says plainly when a probe found nothing", async () => {
    // An empty result is a real outcome, not a silent no-op — without this the
    // admin cannot tell a probe that found nothing from a broken button.
    deviceService.identify.mockResolvedValue({
      ports_probed: [4003, 6053, 6668, 8009],
      answering_ports: [],
      device: {},
    });

    const { result } = renderHook(() => useIdentifyDevice(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("d1");
    });

    expect(toast.success).toHaveBeenCalledWith(
      "No known ports answered (tried 4)",
    );
  });

  it("names the reason when the daemon refuses a probe as offline", async () => {
    // The 409 is reachable in normal use: the button's enabled state comes from
    // `last_seen` at render time and the detail query does not poll. Reporting
    // a bare failure would hide something the admin can act on.
    deviceService.identify.mockRejectedValue(
      Object.assign(new Error("conflict"), { status: 409 }),
    );

    const { result } = renderHook(() => useIdentifyDevice(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await expect(result.current.mutateAsync("d1")).rejects.toThrow();
    });

    expect(toast.error).toHaveBeenCalledWith(
      "This device is no longer on the network, so Wardnet did not contact it",
    );
  });

  it("falls back to a generic message for any other probe failure", async () => {
    deviceService.identify.mockRejectedValue(new Error("boom"));

    const { result } = renderHook(() => useIdentifyDevice(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await expect(result.current.mutateAsync("d1")).rejects.toThrow();
    });

    expect(toast.error).toHaveBeenCalledWith("Failed to identify device");
  });

  it("reads and updates DNS capture settings", async () => {
    deviceService.getDnsCaptureSettings.mockResolvedValue({ enabled: false });
    deviceService.updateDnsCaptureSettings.mockResolvedValue({});
    const { result: read } = renderHook(() => useDnsCaptureSettings("d1"), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(read.current.isSuccess).toBe(true));
    expect(deviceService.getDnsCaptureSettings).toHaveBeenCalledWith("d1");

    const { result: write } = renderHook(
      () => useUpdateDnsCaptureSettings("d1"),
      { wrapper: createQueryWrapper() },
    );
    await act(async () => {
      await write.current.mutateAsync({ enabled: true });
    });
    expect(deviceService.updateDnsCaptureSettings).toHaveBeenCalledWith("d1", {
      enabled: true,
    });
    expect(toast.success).toHaveBeenCalledWith("DNS capture settings saved");
  });

  it("releases a device and reports success", async () => {
    deviceService.release.mockResolvedValue({});
    const { result } = renderHook(() => useReleaseDevice(), {
      wrapper: createQueryWrapper(),
    });

    await act(async () => {
      await result.current.mutateAsync("d1");
    });

    expect(deviceService.release).toHaveBeenCalledWith("d1");
    expect(toast.success).toHaveBeenCalledWith("Device released");
  });

  it("surfaces a failed release rather than reporting success", async () => {
    // A release revokes credentials; silently swallowing the failure would
    // leave the admin believing the device was released when it still holds
    // its Private-DNS grant and Remote peer credential.
    deviceService.release.mockRejectedValue(new Error("boom"));
    const { result } = renderHook(() => useReleaseDevice(), {
      wrapper: createQueryWrapper(),
    });

    await act(async () => {
      await expect(result.current.mutateAsync("d1")).rejects.toThrow("boom");
    });

    expect(toast.error).toHaveBeenCalledWith("Failed to release device");
    expect(toast.success).not.toHaveBeenCalled();
  });
});
