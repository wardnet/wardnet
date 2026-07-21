import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

import type { DnsStatsSnapshot } from "../../src/lib/dnsStats";
import { useDnsStats } from "../../src/hooks/useDnsStats";

// Control loadDnsStats' resolution order so we can force a stale in-flight read
// for date A to settle *after* the user has already switched to date B.
const { loadDnsStats } = vi.hoisted(() => ({ loadDnsStats: vi.fn() }));
vi.mock("../../src/lib/dnsStats", () => ({ loadDnsStats }));

function snapshot(blocked: number): DnsStatsSnapshot {
  return {
    headline: { total: blocked, blocked, allowed: 0 },
    topBlocked: [],
    topQueried: [],
    recent: [],
    trend: [],
    hasData: true,
  };
}

describe("useDnsStats — stale-date race", () => {
  it("ignores a previous day's load that resolves after the day switched", async () => {
    // Deferred promises: one per selected date, resolved by hand below.
    let resolveA!: (s: DnsStatsSnapshot) => void;
    let resolveB!: (s: DnsStatsSnapshot) => void;
    const pendingA = new Promise<DnsStatsSnapshot>((r) => {
      resolveA = r;
    });
    const pendingB = new Promise<DnsStatsSnapshot>((r) => {
      resolveB = r;
    });

    loadDnsStats.mockResolvedValue(snapshot(0));
    loadDnsStats.mockReturnValueOnce(pendingA).mockReturnValueOnce(pendingB);

    const { result, rerender } = renderHook(({ d }) => useDnsStats(d), {
      initialProps: { d: "2026-07-01" },
    });

    // Switch to date B while A's read is still in flight.
    rerender({ d: "2026-07-02" });

    // B resolves first and is committed.
    resolveB(snapshot(2));
    await waitFor(() => expect(result.current.headline.blocked).toBe(2));

    // The stale A read now resolves last. Flush its promise chain inside act so
    // that if the success path were unguarded, the commit WOULD land in
    // result.current — the assertion below proves it does not.
    await act(async () => {
      resolveA(snapshot(9));
      await pendingA;
      await Promise.resolve();
    });
    expect(result.current.headline.blocked).toBe(2);
  });
});
