import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { infoService } = vi.hoisted(() => ({
  infoService: { getInfo: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ infoService }));

import { useDaemonStatus } from "../../src/hooks/useDaemonStatus";

describe("useDaemonStatus", () => {
  beforeEach(() => vi.clearAllMocks());

  it("maps a successful /api/info probe to reachable state", async () => {
    infoService.getInfo.mockResolvedValue({
      release_version: "2026.07.02",
      version: "1.2.3-dev",
      uptime_seconds: 42,
    });
    const { result } = renderHook(() => useDaemonStatus(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({
      reachable: true,
      version: "2026.07.02",
      buildVersion: "1.2.3-dev",
      uptimeSeconds: 42,
    });
  });

  it("maps a failed probe to an unreachable snapshot", async () => {
    infoService.getInfo.mockRejectedValue(new Error("down"));
    const { result } = renderHook(() => useDaemonStatus(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({
      reachable: false,
      version: null,
      buildVersion: null,
      uptimeSeconds: null,
    });
  });
});
