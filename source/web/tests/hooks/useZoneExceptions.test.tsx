import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { zoneExceptionsService, toast } = vi.hoisted(() => ({
  zoneExceptionsService: {
    list: vi.fn(),
    create: vi.fn(),
    delete: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ zoneExceptionsService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import * as z from "../../src/hooks/useZoneExceptions";

const w = createQueryWrapper;

describe("useZoneExceptions", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists exceptions", async () => {
    zoneExceptionsService.list.mockResolvedValue({ exceptions: [] });
    const { result } = renderHook(() => z.useZoneExceptions(), {
      wrapper: w(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
  });

  it("creates and deletes an exception", async () => {
    zoneExceptionsService.create.mockResolvedValue({ exception: {} });
    zoneExceptionsService.delete.mockResolvedValue({ deleted: true });

    const { result: c } = renderHook(() => z.useCreateZoneException(), {
      wrapper: w(),
    });
    await act(async () => {
      await c.current.mutateAsync({
        from: { kind: "device", id: "d1" },
        to: { kind: "zone", id: "z2" },
        service: { type: "preset", set: "casting" },
        bidirectional: true,
      });
    });
    expect(toast.success).toHaveBeenCalledWith("Exception created");

    const { result: d } = renderHook(() => z.useDeleteZoneException(), {
      wrapper: w(),
    });
    await act(async () => {
      await d.current.mutateAsync("e1");
    });
    expect(toast.success).toHaveBeenCalledWith("Exception removed");
  });

  it("surfaces an error toast when create fails", async () => {
    zoneExceptionsService.create.mockRejectedValue(new Error("nope"));
    const { result } = renderHook(() => z.useCreateZoneException(), {
      wrapper: w(),
    });
    await act(async () => {
      await result.current
        .mutateAsync({
          from: { kind: "zone", id: "z1" },
          to: { kind: "zone", id: "z2" },
          service: { type: "preset", set: "casting" },
          bidirectional: false,
        })
        .catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Failed to create exception");
  });

  it("surfaces an error toast when delete fails", async () => {
    zoneExceptionsService.delete.mockRejectedValue(new Error("nope"));
    const { result } = renderHook(() => z.useDeleteZoneException(), {
      wrapper: w(),
    });
    await act(async () => {
      await result.current.mutateAsync("e1").catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Failed to remove exception");
  });
});
