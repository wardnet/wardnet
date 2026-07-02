import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { tunnelService, jobsService, toast } = vi.hoisted(() => ({
  tunnelService: {
    list: vi.fn(),
    getById: vi.fn(),
    listDevices: vi.fn(),
    create: vi.fn(),
    delete: vi.fn(),
    test: vi.fn(),
    rebuild: vi.fn(),
    setDnsOverride: vi.fn(),
    getSpeedTestResults: vi.fn(),
    startSpeedTest: vi.fn(),
  },
  jobsService: { get: vi.fn() },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ tunnelService, jobsService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import {
  useTunnels,
  useTunnel,
  useTunnelDevices,
  useCreateTunnel,
  useDeleteTunnel,
  useTestTunnel,
  useRebuildTunnel,
  useSetTunnelDnsOverride,
  useSpeedTestResults,
  useStartSpeedTest,
} from "../../src/hooks/useTunnels";

const wrapper = createQueryWrapper;

describe("useTunnels queries", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists tunnels", async () => {
    tunnelService.list.mockResolvedValue({ tunnels: [] });
    const { result } = renderHook(() => useTunnels(), { wrapper: wrapper() });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
  });

  it("reads one tunnel + its devices + speed-test results by id", async () => {
    tunnelService.getById.mockResolvedValue({ id: "t1" });
    tunnelService.listDevices.mockResolvedValue({ devices: [] });
    tunnelService.getSpeedTestResults.mockResolvedValue({ results: [] });
    const { result: t } = renderHook(() => useTunnel("t1"), {
      wrapper: wrapper(),
    });
    const { result: d } = renderHook(() => useTunnelDevices("t1"), {
      wrapper: wrapper(),
    });
    const { result: r } = renderHook(() => useSpeedTestResults("t1"), {
      wrapper: wrapper(),
    });
    await waitFor(() => expect(t.current.isSuccess).toBe(true));
    await waitFor(() => expect(d.current.isSuccess).toBe(true));
    await waitFor(() => expect(r.current.isSuccess).toBe(true));
  });

  it("skips fetching a tunnel with an empty id", () => {
    const { result } = renderHook(() => useTunnel(""), { wrapper: wrapper() });
    expect(result.current.fetchStatus).toBe("idle");
  });
});

describe("useTunnels mutations", () => {
  beforeEach(() => vi.clearAllMocks());

  it("creates and deletes a tunnel", async () => {
    tunnelService.create.mockResolvedValue({ message: "created" });
    tunnelService.delete.mockResolvedValue({ message: "deleted" });
    const { result: c } = renderHook(() => useCreateTunnel(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await c.current.mutateAsync({ label: "x" } as never);
    });
    expect(toast.success).toHaveBeenCalledWith("created");

    const { result: d } = renderHook(() => useDeleteTunnel(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await d.current.mutateAsync("t1");
    });
    expect(toast.success).toHaveBeenCalledWith("deleted");
  });

  it("shows a rate-limit toast on a conflicting test", async () => {
    tunnelService.test.mockRejectedValue(new Error("conflict: already"));
    const { result } = renderHook(() => useTestTunnel(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("t1").catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Test already in progress");
  });

  it("shows the raw error for a non-conflict test failure", async () => {
    tunnelService.test.mockRejectedValue(new Error("boom"));
    const { result } = renderHook(() => useTestTunnel(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("t1").catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("boom");
  });

  it("rebuilds a tunnel", async () => {
    tunnelService.rebuild.mockResolvedValue({});
    const { result } = renderHook(() => useRebuildTunnel(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("t1");
    });
    expect(toast.success).toHaveBeenCalledWith("Tunnel rebuild initiated");
  });

  it("toggles the DNS override on and off with distinct copy", async () => {
    tunnelService.setDnsOverride.mockResolvedValue({});
    const { result } = renderHook(() => useSetTunnelDnsOverride(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ id: "t1", value: true });
    });
    expect(tunnelService.setDnsOverride).toHaveBeenCalledWith("t1", {
      override_default_dns: true,
    });
    expect(toast.success).toHaveBeenCalledWith(
      "Tunneled-device DNS will route through wardnet (filtered)",
    );
    await act(async () => {
      await result.current.mutateAsync({ id: "t1", value: false });
    });
    expect(toast.success).toHaveBeenCalledWith(
      "Tunneled-device DNS now uses the system upstream pool",
    );
  });
});

describe("useStartSpeedTest", () => {
  beforeEach(() => vi.clearAllMocks());

  it("dispatches a run and clears the active job on success", async () => {
    tunnelService.startSpeedTest.mockResolvedValue({ job_id: "job-1" });
    jobsService.get.mockResolvedValue({
      id: "job-1",
      status: "SUCCEED",
      percentage_done: 100,
    });
    const { result } = renderHook(() => useStartSpeedTest(), {
      wrapper: wrapper(),
    });
    act(() => {
      result.current.start("t1");
    });
    await waitFor(() =>
      expect(tunnelService.startSpeedTest).toHaveBeenCalledWith("t1"),
    );
    await waitFor(() => expect(result.current.activeTunnelId).toBeNull());
  });

  it("surfaces a conflict toast when a run is already in progress", async () => {
    tunnelService.startSpeedTest.mockRejectedValue(
      new Error("already running"),
    );
    const { result } = renderHook(() => useStartSpeedTest(), {
      wrapper: wrapper(),
    });
    act(() => {
      result.current.start("t1");
    });
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        "Speed test already in progress",
      ),
    );
  });
});
