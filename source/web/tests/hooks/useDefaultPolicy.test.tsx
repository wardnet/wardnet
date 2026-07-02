import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { systemService } = vi.hoisted(() => ({
  systemService: { getDefaultPolicy: vi.fn(), setDefaultPolicy: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ systemService }));

import {
  useDefaultPolicy,
  useSetDefaultPolicy,
} from "../../src/hooks/useDefaultPolicy";

describe("useDefaultPolicy", () => {
  beforeEach(() => vi.clearAllMocks());

  it("reads the current default policy", async () => {
    systemService.getDefaultPolicy.mockResolvedValue({ policy: "direct" });
    const { result } = renderHook(() => useDefaultPolicy(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ policy: "direct" });
  });

  it("updates the default policy", async () => {
    systemService.setDefaultPolicy.mockResolvedValue({});
    const { result } = renderHook(() => useSetDefaultPolicy(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("tunnel-uuid");
    });
    expect(systemService.setDefaultPolicy).toHaveBeenCalledWith({
      policy: "tunnel-uuid",
    });
  });
});
