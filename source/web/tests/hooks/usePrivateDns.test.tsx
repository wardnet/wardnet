import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { privateDnsService, toast } = vi.hoisted(() => ({
  privateDnsService: {
    getStatus: vi.fn(),
    setEnabled: vi.fn(),
    grantDevice: vi.fn(),
    revokeDevice: vi.fn(),
    notifyDevice: vi.fn(),
    me: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ privateDnsService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import {
  usePrivateDnsStatus,
  useSetPrivateDnsEnabled,
  useGrantPrivateDnsDevice,
  useRevokePrivateDnsDevice,
  useSendPrivateDnsToDevice,
  usePrivateDnsMe,
} from "../../src/hooks/usePrivateDns";

const STATUS = {
  enabled: false,
  domain: "abc.my.wardnet.services",
  prerequisites: {
    wardnet_provider: true,
    certificate: true,
    entitled: true,
    relay_connected: false,
  },
  grants: [],
};

describe("usePrivateDns hooks", () => {
  beforeEach(() => vi.clearAllMocks());

  it("reads status and the device-keyed me view", async () => {
    privateDnsService.getStatus.mockResolvedValue(STATUS);
    privateDnsService.me.mockResolvedValue({
      enabled: true,
      granted: true,
      hostname: "tok.abc.my.wardnet.services",
    });

    const { result: s } = renderHook(() => usePrivateDnsStatus(), {
      wrapper: createQueryWrapper(),
    });
    const { result: m } = renderHook(() => usePrivateDnsMe(), {
      wrapper: createQueryWrapper(),
    });

    await waitFor(() => expect(s.current.isSuccess).toBe(true));
    await waitFor(() => expect(m.current.isSuccess).toBe(true));
    expect(s.current.data).toEqual(STATUS);
    expect(m.current.data?.hostname).toBe("tok.abc.my.wardnet.services");
  });

  it("enables Private DNS and toasts success", async () => {
    privateDnsService.setEnabled.mockResolvedValueOnce({
      ...STATUS,
      enabled: true,
    });
    const { result } = renderHook(() => useSetPrivateDnsEnabled(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync(true);
    });
    expect(privateDnsService.setEnabled).toHaveBeenCalledWith(true);
    expect(toast.success).toHaveBeenCalledWith("Private DNS enabled");
  });

  it("shows a specific message when enabling 409s on a missing prerequisite", async () => {
    privateDnsService.setEnabled.mockRejectedValueOnce({ status: 409 });
    const { result } = renderHook(() => useSetPrivateDnsEnabled(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await expect(result.current.mutateAsync(true)).rejects.toBeTruthy();
    });
    expect(toast.error).toHaveBeenCalledWith(
      expect.stringContaining("prerequisite"),
    );
  });

  it("grants a device", async () => {
    privateDnsService.grantDevice.mockResolvedValueOnce({
      id: "g1",
      device_id: "d1",
      hostname: "tok.abc.my.wardnet.services",
      created_at: "2026-07-21T00:00:00Z",
    });
    const { result } = renderHook(() => useGrantPrivateDnsDevice(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("d1");
    });
    expect(privateDnsService.grantDevice).toHaveBeenCalledWith("d1");
  });

  it("revokes a device by device id and toasts", async () => {
    privateDnsService.revokeDevice.mockResolvedValueOnce(undefined);
    const { result } = renderHook(() => useRevokePrivateDnsDevice(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("d1");
    });
    expect(privateDnsService.revokeDevice).toHaveBeenCalledWith("d1");
    expect(toast.success).toHaveBeenCalledWith("Private DNS access revoked");
  });

  it("send-to-device: success toast when delivered", async () => {
    privateDnsService.notifyDevice.mockResolvedValueOnce({ delivered: true });
    const { result } = renderHook(() => useSendPrivateDnsToDevice(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("d1");
    });
    expect(privateDnsService.notifyDevice).toHaveBeenCalledWith("d1");
    expect(toast.success).toHaveBeenCalledWith("Sent to device");
    expect(toast.warning).not.toHaveBeenCalled();
  });

  it("send-to-device: warns when the device has no subscription", async () => {
    privateDnsService.notifyDevice.mockResolvedValueOnce({ delivered: false });
    const { result } = renderHook(() => useSendPrivateDnsToDevice(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("d1");
    });
    expect(toast.warning).toHaveBeenCalledWith(
      expect.stringContaining("hasn't enabled notifications"),
    );
    expect(toast.success).not.toHaveBeenCalled();
  });
});
