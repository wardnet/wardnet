import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { statsService } = vi.hoisted(() => ({
  statsService: { query: vi.fn(), top: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ statsService }));

import {
  RANGES,
  RANGE_HOURS,
  parseLabels,
  useDnsStatSummary,
  useDnsStatsDashboard,
  useDashboardDnsStats,
  useDnsTopBlockedDomains,
} from "../../src/hooks/useStats";

const wrapper = createQueryWrapper;

function seriesPoint(ts: string, value: number, outcome?: string) {
  return {
    ts,
    value,
    labels: outcome ? JSON.stringify({ outcome }) : "{}",
  };
}

describe("stats helpers", () => {
  it("exposes range metadata", () => {
    expect(RANGES.map((r) => r.value)).toContain("24h");
    expect(RANGE_HOURS["7d"]).toBe(168);
  });

  it("parseLabels tolerates invalid JSON", () => {
    expect(parseLabels('{"outcome":"blocked"}')).toEqual({
      outcome: "blocked",
    });
    expect(parseLabels("not json")).toEqual({});
  });
});

describe("useDnsStatSummary", () => {
  beforeEach(() => vi.clearAllMocks());

  it("aggregates total, blocked and blocked percentage", async () => {
    statsService.query.mockResolvedValue({
      series: [
        seriesPoint("2026-07-02T00:00:00Z", 100),
        seriesPoint("2026-07-02T00:00:00Z", 25, "blocked"),
      ],
    });
    const { result } = renderHook(() => useDnsStatSummary("24h"), {
      wrapper: wrapper(),
    });
    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(result.current.data).toEqual({
      total: 125,
      blocked: 25,
      blockedPercent: 20,
    });
  });
});

describe("useDnsStatsDashboard", () => {
  beforeEach(() => vi.clearAllMocks());

  it("merges the series and top-N queries into one bundle", async () => {
    statsService.query.mockResolvedValue({
      series: [
        seriesPoint("2026-07-02T00:00:00Z", 40),
        seriesPoint("2026-07-02T00:00:00Z", 10, "blocked"),
        seriesPoint("2026-07-02T01:00:00Z", 50),
      ],
    });
    statsService.top.mockImplementation((req: { metric: string }) =>
      Promise.resolve({
        metric: req.metric,
        entries: [{ key: "a", value: 1 }],
      }),
    );
    const { result } = renderHook(() => useDnsStatsDashboard("24h"), {
      wrapper: wrapper(),
    });
    await waitFor(() => expect(result.current.data).toBeDefined());
    const data = result.current.data!;
    expect(data.total).toBe(100);
    expect(data.blocked).toBe(10);
    expect(data.series).toHaveLength(2);
    expect(data.topDomains.entries).toHaveLength(1);
    expect(data.topClients.entries).toHaveLength(1);
  });

  it("uses the top-override window for the top-N queries", async () => {
    statsService.query.mockResolvedValue({ series: [] });
    statsService.top.mockResolvedValue({ metric: "m", entries: [] });
    renderHook(
      () =>
        useDnsStatsDashboard("24h", {
          from: "2026-07-01T00:00:00Z",
          to: "2026-07-01T06:00:00Z",
        }),
      { wrapper: wrapper() },
    );
    await waitFor(() => expect(statsService.top).toHaveBeenCalled());
    const call = statsService.top.mock.calls[0][0];
    expect(call.from).toBe("2026-07-01T00:00:00Z");
    expect(call.to).toBe("2026-07-01T06:00:00Z");
  });
});

describe("useDashboardDnsStats", () => {
  beforeEach(() => vi.clearAllMocks());

  it("builds oldest-first total and blocked series", async () => {
    statsService.query.mockResolvedValue({
      series: [
        seriesPoint("2026-07-02T01:00:00Z", 30),
        seriesPoint("2026-07-02T00:00:00Z", 20),
        seriesPoint("2026-07-02T00:00:00Z", 5, "blocked"),
      ],
    });
    const { result } = renderHook(() => useDashboardDnsStats(), {
      wrapper: wrapper(),
    });
    await waitFor(() => expect(result.current.data).toBeDefined());
    const data = result.current.data!;
    expect(data.total).toBe(55);
    expect(data.blocked).toBe(5);
    expect(data.totalSeries).toEqual([25, 30]); // sorted by ts ascending
    expect(data.blockedSeries).toEqual([5, 0]);
  });
});

describe("useDnsTopBlockedDomains", () => {
  beforeEach(() => vi.clearAllMocks());

  it("queries the by-domain metric with the given limit", async () => {
    statsService.top.mockResolvedValue({ metric: "m", entries: [] });
    const { result } = renderHook(() => useDnsTopBlockedDomains(5), {
      wrapper: wrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(statsService.top.mock.calls[0][0]).toMatchObject({
      metric: "dns.queries.by_domain",
      label_key: "domain",
      limit: 5,
    });
  });
});
