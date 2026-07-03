import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { statsService } = vi.hoisted(() => ({
  statsService: { queryMulti: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ statsService }));

import { useTunnelStats } from "../../src/hooks/useTunnelStats";
import { useCombinedTunnelStats } from "../../src/hooks/useCombinedTunnelStats";

const wrapper = createQueryWrapper;

describe("useTunnelStats", () => {
  beforeEach(() => vi.clearAllMocks());

  it("skips the query for an empty tunnel id", () => {
    const { result } = renderHook(() => useTunnelStats("", "1h"), {
      wrapper: wrapper(),
    });
    expect(statsService.queryMulti).not.toHaveBeenCalled();
    expect(result.current.data).toBeUndefined();
  });

  it("merges tx/rx/rtt series into one timeline sorted by bucket", async () => {
    statsService.queryMulti.mockResolvedValue({
      "tunnel.bytes.tx": [{ ts: "2026-07-02T00:01:00Z", value: 100 }],
      "tunnel.bytes.rx": [{ ts: "2026-07-02T00:00:00Z", value: 50 }],
      "tunnel.latency.rtt_ms": [{ ts: "2026-07-02T00:00:00Z", value: 12 }],
    });
    const { result } = renderHook(() => useTunnelStats("t1", "1h"), {
      wrapper: wrapper(),
    });
    await waitFor(() => expect(result.current.data).toBeDefined());
    const pts = result.current.data!.points;
    expect(pts).toHaveLength(2);
    expect(pts[0]).toMatchObject({ bytesRx: 50, rttMs: 12 });
    expect(pts[1]).toMatchObject({ bytesTx: 100, rttMs: null });
    expect(result.current.data!.bucketSecs).toBe(60);
    // label filter passes a canonical single-key JSON string
    expect(statsService.queryMulti.mock.calls[0][0].label_filter).toBe(
      JSON.stringify({ tunnel_id: "t1" }),
    );
  });
});

describe("useCombinedTunnelStats", () => {
  beforeEach(() => vi.clearAllMocks());

  it("returns undefined data before the query resolves shape", async () => {
    statsService.queryMulti.mockResolvedValue({
      "tunnel.bytes.tx": [
        { ts: "2026-07-02T00:00:00Z", value: 120 },
        { ts: "2026-07-02T00:01:00Z", value: 60 },
      ],
      "tunnel.bytes.rx": [{ ts: "2026-07-02T00:01:00Z", value: 30 }],
    });
    const { result } = renderHook(() => useCombinedTunnelStats(), {
      wrapper: wrapper(),
    });
    await waitFor(() => expect(result.current.data).toBeDefined());
    const data = result.current.data!;
    expect(data.sparkValues).toEqual([120, 90]); // per-bucket tx+rx, ascending
    expect(data.txRate).toBe(1); // last bucket tx 60 / 60s
    expect(data.rxRate).toBe(0.5); // last bucket rx 30 / 60s
  });
});
