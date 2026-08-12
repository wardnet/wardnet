import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { anomalyService } = vi.hoisted(() => ({
  anomalyService: {
    list: vi.fn(),
    reevaluate: vi.fn(),
  },
}));
vi.mock("../../src/lib/sdk", () => ({ anomalyService }));

import {
  useAnomalies,
  useReevaluateAnomalies,
} from "../../src/hooks/useAnomalies";

describe("useAnomalies", () => {
  beforeEach(() => vi.clearAllMocks());

  it("fetches the open anomalies by default", async () => {
    anomalyService.list.mockResolvedValue({ anomalies: [] });
    const { result } = renderHook(() => useAnomalies(), {
      wrapper: createQueryWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    // No filter is pinned client-side: the daemon's defaults are what a bare
    // call should get.
    expect(anomalyService.list).toHaveBeenCalledWith({});
  });

  it("passes filters through to the service", async () => {
    anomalyService.list.mockResolvedValue({ anomalies: [] });
    const { result } = renderHook(
      () => useAnomalies({ subject_id: "tun-1", status: "all" }),
      { wrapper: createQueryWrapper() },
    );

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(anomalyService.list).toHaveBeenCalledWith({
      subject_id: "tun-1",
      status: "all",
    });
  });

  it("reevaluates on demand", async () => {
    anomalyService.reevaluate.mockResolvedValue({ evaluated: 3, resolved: 1 });
    const { result } = renderHook(() => useReevaluateAnomalies(), {
      wrapper: createQueryWrapper(),
    });

    await act(async () => {
      await result.current.mutateAsync();
    });
    expect(anomalyService.reevaluate).toHaveBeenCalledOnce();
  });
});
