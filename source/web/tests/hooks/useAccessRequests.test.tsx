import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { accessRequestService, toast } = vi.hoisted(() => ({
  accessRequestService: {
    listMine: vi.fn(),
    createMine: vi.fn(),
    list: vi.fn(),
    decide: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ accessRequestService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import {
  useMyAccessRequests,
  useCreateAccessRequest,
  useAccessRequests,
  useDecideAccessRequest,
} from "../../src/hooks/useAccessRequests";

describe("useAccessRequests hooks", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists the caller's own requests", async () => {
    accessRequestService.listMine.mockResolvedValue([]);
    const { result } = renderHook(() => useMyAccessRequests(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(accessRequestService.listMine).toHaveBeenCalledOnce();
  });

  it("creates a request and toasts success", async () => {
    accessRequestService.createMine.mockResolvedValue({});
    const { result } = renderHook(() => useCreateAccessRequest(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ kind: "block", domain: "x.com" });
    });
    expect(toast.success).toHaveBeenCalledWith(
      "Request sent to your administrator",
    );
  });

  it("creates a Private DNS request with no domain", async () => {
    accessRequestService.createMine.mockResolvedValue({});
    const { result } = renderHook(() => useCreateAccessRequest(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ kind: "private_dns" });
    });
    expect(accessRequestService.createMine).toHaveBeenCalledWith({
      kind: "private_dns",
    });
  });

  it("lists all requests filtered by status", async () => {
    accessRequestService.list.mockResolvedValue([]);
    const { result } = renderHook(() => useAccessRequests("pending"), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(accessRequestService.list).toHaveBeenCalledWith("pending");
  });

  it("approves a request", async () => {
    accessRequestService.decide.mockResolvedValue({ status: "approved" });
    const { result } = renderHook(() => useDecideAccessRequest(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ id: "r1", status: "approved" });
    });
    expect(accessRequestService.decide).toHaveBeenCalledWith(
      "r1",
      "approved",
      undefined,
    );
    expect(toast.success).toHaveBeenCalledWith("Request approved");
  });

  it("passes kind-specific approval params through", async () => {
    accessRequestService.decide.mockResolvedValue({ status: "approved" });
    const { result } = renderHook(() => useDecideAccessRequest(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({
        id: "r1",
        status: "approved",
        approval: { kind: "private_dns" },
      });
    });
    expect(accessRequestService.decide).toHaveBeenCalledWith("r1", "approved", {
      kind: "private_dns",
    });
  });

  it("declines a request", async () => {
    accessRequestService.decide.mockResolvedValue({ status: "rejected" });
    const { result } = renderHook(() => useDecideAccessRequest(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ id: "r1", status: "rejected" });
    });
    expect(toast.success).toHaveBeenCalledWith("Request declined");
  });

  // The listener can resolve the row to `approved` from an out-of-band grant
  // while the admin is clicking Decline; the toast must report what landed.
  it("reports the decision the server returned, not the one requested", async () => {
    accessRequestService.decide.mockResolvedValue({ status: "approved" });
    const { result } = renderHook(() => useDecideAccessRequest(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ id: "r1", status: "rejected" });
    });
    expect(toast.success).toHaveBeenCalledWith("Request approved");
  });

  it("toasts an error when a decision fails", async () => {
    accessRequestService.decide.mockRejectedValue(new Error("x"));
    const { result } = renderHook(() => useDecideAccessRequest(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current
        .mutateAsync({ id: "r1", status: "approved" })
        .catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Failed to update request");
  });
});
