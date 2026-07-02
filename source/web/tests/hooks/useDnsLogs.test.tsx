import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { dnsService } = vi.hoisted(() => ({
  dnsService: { listQueryLog: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ dnsService }));

import { useDnsQueryLog } from "../../src/hooks/useDnsLogs";

describe("useDnsQueryLog", () => {
  beforeEach(() => vi.clearAllMocks());

  it("fetches the paginated query log with the given filter", async () => {
    dnsService.listQueryLog.mockResolvedValue({ entries: [], total: 0 });
    const filter = { limit: 50, offset: 0 };
    const { result } = renderHook(() => useDnsQueryLog(filter), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(dnsService.listQueryLog).toHaveBeenCalledWith(filter);
  });
});
