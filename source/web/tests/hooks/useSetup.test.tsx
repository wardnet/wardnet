import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { setupService } = vi.hoisted(() => ({
  setupService: { getStatus: vi.fn(), setup: vi.fn(), advance: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ setupService }));

import {
  useSetupStatus,
  useSetup,
  useAdvanceWizard,
} from "../../src/hooks/useSetup";

describe("useSetup hooks", () => {
  beforeEach(() => vi.clearAllMocks());

  it("reads the wizard status", async () => {
    setupService.getStatus.mockResolvedValue({ step: "welcome" });
    const { result } = renderHook(() => useSetupStatus(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ step: "welcome" });
  });

  it("creates the first admin account", async () => {
    setupService.setup.mockResolvedValue({});
    const { result } = renderHook(() => useSetup(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ username: "admin", password: "pw" });
    });
    expect(setupService.setup).toHaveBeenCalledWith({
      username: "admin",
      password: "pw",
    });
  });

  it("advances the wizard", async () => {
    setupService.advance.mockResolvedValue({});
    const { result } = renderHook(() => useAdvanceWizard(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ step: "network" } as never);
    });
    expect(setupService.advance).toHaveBeenCalledWith({ step: "network" });
  });
});
