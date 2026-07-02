import { renderHook, act, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WardnetApiError } from "@wardnet/js";

const { systemService } = vi.hoisted(() => ({
  systemService: {
    restart: vi.fn(),
    reboot: vi.fn(),
    shutdown: vi.fn(),
    getStatus: vi.fn(),
  },
}));
vi.mock("../../src/lib/sdk", () => ({ systemService }));

import { useRestart } from "../../src/hooks/useRestart";
import { useReboot } from "../../src/hooks/useReboot";
import { useShutdown } from "../../src/hooks/useShutdown";

describe("power-op hooks", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: true }));
  });
  afterEach(() => vi.unstubAllGlobals());

  it.each([
    ["restart", useRestart, "restart"],
    ["reboot", useReboot, "reboot"],
    ["shutdown", useShutdown, "shutdown"],
  ] as const)(
    "%s: fires the endpoint and enters scheduled on a 204",
    async (_n, hook, method) => {
      (systemService[method] as ReturnType<typeof vi.fn>).mockResolvedValue(
        undefined,
      );
      const { result } = renderHook(() => hook());
      act(() => result.current.start());
      await waitFor(() => expect(systemService[method]).toHaveBeenCalledOnce());
      expect(result.current.phase).toBe("scheduled");
      expect(result.current.isOpen).toBe(true);
      // Tear down the poll loop the hook started.
      act(() => result.current.reset());
      expect(result.current.phase).toBe("idle");
    },
  );

  it("restart: surfaces the API error detail when the POST rejects", async () => {
    systemService.restart.mockRejectedValue(
      new WardnetApiError(500, "Server Error", {
        error: "err",
        detail: "polkit missing",
      }),
    );
    const { result } = renderHook(() => useRestart());
    act(() => result.current.start());
    await waitFor(() => expect(result.current.phase).toBe("failed"));
    expect(result.current.errorMessage).toBe("polkit missing");
  });

  it("reboot: falls back to a generic message on a non-API error", async () => {
    systemService.reboot.mockRejectedValue("weird");
    const { result } = renderHook(() => useReboot());
    act(() => result.current.start());
    await waitFor(() => expect(result.current.phase).toBe("failed"));
    expect(result.current.errorMessage).toBe("Failed to reboot");
  });

  it("shutdown: uses Error.message when the POST throws a plain Error", async () => {
    systemService.shutdown.mockRejectedValue(new Error("boom"));
    const { result } = renderHook(() => useShutdown());
    act(() => result.current.start());
    await waitFor(() => expect(result.current.phase).toBe("failed"));
    expect(result.current.errorMessage).toBe("boom");
  });
});
