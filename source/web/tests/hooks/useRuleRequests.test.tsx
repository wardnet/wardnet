import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { ruleRequestService, toast } = vi.hoisted(() => ({
  ruleRequestService: {
    listMine: vi.fn(),
    createMine: vi.fn(),
    list: vi.fn(),
    decide: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ ruleRequestService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import {
  useMyRuleRequests,
  useCreateRuleRequest,
  useRuleRequests,
  useDecideRuleRequest,
} from "../../src/hooks/useRuleRequests";

describe("useRuleRequests hooks", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists the caller's own requests", async () => {
    ruleRequestService.listMine.mockResolvedValue({ requests: [] });
    const { result } = renderHook(() => useMyRuleRequests(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(ruleRequestService.listMine).toHaveBeenCalledOnce();
  });

  it("creates a request and toasts success", async () => {
    ruleRequestService.createMine.mockResolvedValue({});
    const { result } = renderHook(() => useCreateRuleRequest(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({
        domain: "x.com",
        action: "block",
      } as never);
    });
    expect(toast.success).toHaveBeenCalledWith(
      "Request sent to your administrator",
    );
  });

  it("lists all requests filtered by status", async () => {
    ruleRequestService.list.mockResolvedValue({ requests: [] });
    const { result } = renderHook(() => useRuleRequests("pending"), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(ruleRequestService.list).toHaveBeenCalledWith("pending");
  });

  it("approves a request", async () => {
    ruleRequestService.decide.mockResolvedValue({});
    const { result } = renderHook(() => useDecideRuleRequest(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ id: "r1", status: "approved" });
    });
    expect(ruleRequestService.decide).toHaveBeenCalledWith("r1", "approved");
    expect(toast.success).toHaveBeenCalledWith("Request approved");
  });

  it("rejects a request", async () => {
    ruleRequestService.decide.mockResolvedValue({});
    const { result } = renderHook(() => useDecideRuleRequest(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ id: "r1", status: "rejected" });
    });
    expect(toast.success).toHaveBeenCalledWith("Request rejected");
  });

  it("toasts an error when a decision fails", async () => {
    ruleRequestService.decide.mockRejectedValue(new Error("x"));
    const { result } = renderHook(() => useDecideRuleRequest(), {
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
