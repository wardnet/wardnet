import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { dnsService, toast } = vi.hoisted(() => ({
  dnsService: {
    status: vi.fn(),
    getConfig: vi.fn(),
    toggle: vi.fn(),
    updateConfig: vi.fn(),
    flushCache: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ dnsService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import {
  useDnsStatus,
  useDnsConfig,
  useToggleDns,
  useUpdateDnsConfig,
  useFlushDnsCache,
} from "../../src/hooks/useDns";

describe("useDns hooks", () => {
  beforeEach(() => vi.clearAllMocks());

  it("reads status and config", async () => {
    dnsService.status.mockResolvedValue({ running: true });
    dnsService.getConfig.mockResolvedValue({ upstream: [] });
    const { result: s } = renderHook(() => useDnsStatus(), {
      wrapper: createQueryWrapper(),
    });
    const { result: c } = renderHook(() => useDnsConfig(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(s.current.isSuccess).toBe(true));
    await waitFor(() => expect(c.current.isSuccess).toBe(true));
  });

  it("toggles DNS on and off with the right toast", async () => {
    dnsService.toggle.mockResolvedValueOnce({ config: { enabled: true } });
    const { result } = renderHook(() => useToggleDns(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync(true);
    });
    expect(dnsService.toggle).toHaveBeenCalledWith({ enabled: true });
    // The toast must carry the lease-renewal caveat: DHCP is pull-based, so
    // flipping this does not reach already-leased devices until they renew.
    // Without it the admin believes filtering went network-wide instantly, and
    // silently-unfiltered devices look like a broken filter.
    expect(toast.success).toHaveBeenCalledWith(
      "DNS server enabled",
      expect.objectContaining({
        description: expect.stringContaining("Reconnect a device"),
      }),
    );

    dnsService.toggle.mockResolvedValueOnce({ config: { enabled: false } });
    await act(async () => {
      await result.current.mutateAsync(false);
    });
    expect(toast.success).toHaveBeenCalledWith(
      "DNS server disabled",
      expect.objectContaining({ description: expect.any(String) }),
    );
  });

  it("updates config", async () => {
    dnsService.updateConfig.mockResolvedValue({});
    const { result } = renderHook(() => useUpdateDnsConfig(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ upstream: ["1.1.1.1"] } as never);
    });
    expect(toast.success).toHaveBeenCalledWith("DNS configuration updated");
  });

  it("flushes the cache and reports the count", async () => {
    dnsService.flushCache.mockResolvedValue({ entries_cleared: 12 });
    const { result } = renderHook(() => useFlushDnsCache(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync();
    });
    expect(toast.success).toHaveBeenCalledWith(
      "Cache flushed (12 entries cleared)",
    );
  });

  it("toasts an error on flush failure", async () => {
    dnsService.flushCache.mockRejectedValue(new Error("x"));
    const { result } = renderHook(() => useFlushDnsCache(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync().catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Failed to flush DNS cache");
  });
});
