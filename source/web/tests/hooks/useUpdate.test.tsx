import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WardnetApiError } from "@wardnet/js";
import { createQueryWrapper } from "../test-utils";

const { updateService, toast } = vi.hoisted(() => ({
  updateService: {
    status: vi.fn(),
    history: vi.fn(),
    check: vi.fn(),
    install: vi.fn(),
    rollback: vi.fn(),
    updateConfig: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ updateService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import {
  useUpdateStatus,
  useUpdateHistory,
  useCheckForUpdates,
  useInstallUpdate,
  useRollbackUpdate,
  useUpdateConfig,
} from "../../src/hooks/useUpdate";

const wrapper = createQueryWrapper;

describe("useUpdate", () => {
  beforeEach(() => vi.clearAllMocks());

  it("reads status and history", async () => {
    updateService.status.mockResolvedValue({ status: {} });
    updateService.history.mockResolvedValue({ entries: [] });
    const { result: s } = renderHook(() => useUpdateStatus(), {
      wrapper: wrapper(),
    });
    const { result: h } = renderHook(() => useUpdateHistory(5), {
      wrapper: wrapper(),
    });
    await waitFor(() => expect(s.current.isSuccess).toBe(true));
    await waitFor(() => expect(h.current.isSuccess).toBe(true));
    expect(updateService.history).toHaveBeenCalledWith(5);
  });

  it("announces an available update after a check", async () => {
    updateService.check.mockResolvedValue({
      status: { update_available: true, latest_version: "2026.07.03" },
    });
    const { result } = renderHook(() => useCheckForUpdates(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync();
    });
    expect(toast.success).toHaveBeenCalledWith(
      "Update available: v2026.07.03",
      { id: "update-check" },
    );
  });

  it("announces up-to-date when no update is available", async () => {
    updateService.check.mockResolvedValue({
      status: { update_available: false },
    });
    const { result } = renderHook(() => useCheckForUpdates(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync();
    });
    expect(toast.success).toHaveBeenCalledWith("Wardnet is up to date", {
      id: "update-check",
    });
  });

  it("installs an update", async () => {
    updateService.install.mockResolvedValue({
      handle: { target_version: "2026.07.03" },
    });
    const { result } = renderHook(() => useInstallUpdate(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({});
    });
    expect(toast.success).toHaveBeenCalledWith("Installing v2026.07.03...", {
      id: "update-install",
    });
  });

  it("rolls back", async () => {
    updateService.rollback.mockResolvedValue({});
    const { result } = renderHook(() => useRollbackUpdate(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync();
    });
    expect(toast.success).toHaveBeenCalledWith(
      "Rollback staged - daemon will restart",
      { id: "update-rollback" },
    );
  });

  it("saves update config", async () => {
    updateService.updateConfig.mockResolvedValue({ status: { channel: "s" } });
    const { result } = renderHook(() => useUpdateConfig(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ channel: "stable" });
    });
    expect(toast.success).toHaveBeenCalledWith("Update settings saved", {
      id: "update-config",
    });
  });

  it("prefers the API error detail on failure", async () => {
    updateService.check.mockRejectedValue(
      new WardnetApiError(503, "Unavailable", {
        error: "upstream",
        detail: "Upstream down",
      }),
    );
    const { result } = renderHook(() => useCheckForUpdates(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync().catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Upstream down", {
      id: "update-check",
    });
  });
});
