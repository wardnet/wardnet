import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { systemService, client } = vi.hoisted(() => ({
  systemService: { getStatus: vi.fn(), acknowledgeShutdown: vi.fn() },
  client: { request: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ systemService, client }));

import {
  useSystemStatus,
  useAcknowledgeShutdown,
  useRecentErrors,
} from "../../src/hooks/useSystemStatus";

describe("useSystemStatus", () => {
  beforeEach(() => vi.clearAllMocks());

  it("fetches the system status", async () => {
    systemService.getStatus.mockResolvedValue({ ok: true });
    const { result } = renderHook(() => useSystemStatus(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ ok: true });
  });

  it("acknowledges the unclean-shutdown banner", async () => {
    systemService.acknowledgeShutdown.mockResolvedValue({});
    const { result } = renderHook(() => useAcknowledgeShutdown(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync();
    });
    expect(systemService.acknowledgeShutdown).toHaveBeenCalledOnce();
  });

  it("fetches recent errors from the raw client endpoint", async () => {
    client.request.mockResolvedValue({ errors: [] });
    const { result } = renderHook(() => useRecentErrors(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(client.request).toHaveBeenCalledWith("/system/errors");
  });
});
