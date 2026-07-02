import { beforeEach, describe, expect, it } from "vitest";
import { IDBFactory } from "fake-indexeddb";

import { applyEvent, openDb, type DnsEventItem } from "../../src/lib/dnsDb";
import { loadDnsStats } from "../../src/lib/dnsStats";

function ev(overrides: Partial<DnsEventItem> = {}): DnsEventItem {
  return {
    id: 1,
    domain: "example.com",
    status: "forwarded",
    captured_at: "2026-07-02T12:00:00Z",
    ...overrides,
  };
}

async function seed(events: DnsEventItem[]): Promise<void> {
  const db = await openDb();
  for (const e of events) {
    await applyEvent(db, e);
  }
  db.close();
}

beforeEach(() => {
  globalThis.indexedDB = new IDBFactory();
});

describe("loadDnsStats", () => {
  it("returns an empty snapshot with hasData=false when there is no data", async () => {
    const snap = await loadDnsStats("2026-07-02", ["2026-07-01", "2026-07-02"]);
    expect(snap.hasData).toBe(false);
    expect(snap.headline).toEqual({ total: 0, blocked: 0, allowed: 0 });
    expect(snap.topBlocked).toEqual([]);
    expect(snap.topQueried).toEqual([]);
    expect(snap.recent).toEqual([]);
    expect(snap.trend).toEqual([
      { date: "2026-07-01", blocked: 0, allowed: 0 },
      { date: "2026-07-02", blocked: 0, allowed: 0 },
    ]);
  });

  it("aggregates headline totals for the selected day", async () => {
    await seed([
      ev({ id: 1, domain: "a.com", status: "blocked" }),
      ev({ id: 2, domain: "a.com", status: "forwarded" }),
      ev({ id: 3, domain: "b.com", status: "forwarded" }),
    ]);
    const snap = await loadDnsStats("2026-07-02", ["2026-07-02"]);
    expect(snap.hasData).toBe(true);
    expect(snap.headline).toEqual({ total: 3, blocked: 1, allowed: 2 });
  });

  it("ranks topBlocked and topQueried descending, filtering zero counts", async () => {
    await seed([
      // a.com: 3 blocked
      ev({ id: 1, domain: "a.com", status: "blocked" }),
      ev({ id: 2, domain: "a.com", status: "blocked" }),
      ev({ id: 3, domain: "a.com", status: "blocked" }),
      // b.com: 1 blocked, 5 allowed
      ev({ id: 4, domain: "b.com", status: "blocked" }),
      ev({ id: 5, domain: "b.com", status: "forwarded" }),
      ev({ id: 6, domain: "b.com", status: "forwarded" }),
      ev({ id: 7, domain: "b.com", status: "forwarded" }),
      ev({ id: 8, domain: "b.com", status: "forwarded" }),
      ev({ id: 9, domain: "b.com", status: "forwarded" }),
      // c.com: allowed only — excluded from topBlocked (count 0)
      ev({ id: 10, domain: "c.com", status: "cache_hit" }),
    ]);
    const snap = await loadDnsStats("2026-07-02", ["2026-07-02"], 20, 10);

    // topBlocked: a.com(3), b.com(1); c.com filtered (0 blocked)
    expect(snap.topBlocked).toEqual([
      { domain: "a.com", count: 3 },
      { domain: "b.com", count: 1 },
    ]);
    // topQueried by total: b.com(6), a.com(3), c.com(1)
    expect(snap.topQueried[0]).toEqual({ domain: "b.com", count: 6 });
    expect(snap.topQueried[1]).toEqual({ domain: "a.com", count: 3 });
    expect(snap.topQueried[2]).toEqual({ domain: "c.com", count: 1 });
  });

  it("respects the topN limit", async () => {
    const events: DnsEventItem[] = [];
    for (let i = 0; i < 5; i += 1) {
      events.push(
        ev({ id: i + 1, domain: `d${i}.com`, status: "blocked" }),
      );
    }
    await seed(events);
    const snap = await loadDnsStats("2026-07-02", ["2026-07-02"], 20, 2);
    expect(snap.topBlocked).toHaveLength(2);
  });

  it("returns recent activity newest-first, capped at recentLimit", async () => {
    await seed([
      ev({ id: 1, domain: "one.com" }),
      ev({ id: 2, domain: "two.com" }),
      ev({ id: 3, domain: "three.com" }),
    ]);
    const snap = await loadDnsStats("2026-07-02", ["2026-07-02"], 2, 10);
    expect(snap.recent).toHaveLength(2);
    expect(snap.recent[0].id).toBe(3);
    expect(snap.recent[1].id).toBe(2);
  });

  it("computes a multi-day trend keyed by date", async () => {
    await seed([
      ev({ id: 1, captured_at: "2026-07-01T12:00:00Z", status: "blocked" }),
      ev({ id: 2, captured_at: "2026-07-02T12:00:00Z", status: "forwarded" }),
      ev({ id: 3, captured_at: "2026-07-02T13:00:00Z", status: "blocked" }),
    ]);
    const snap = await loadDnsStats("2026-07-02", [
      "2026-07-01",
      "2026-07-02",
      "2026-07-03",
    ]);
    expect(snap.trend).toEqual([
      { date: "2026-07-01", blocked: 1, allowed: 0 },
      { date: "2026-07-02", blocked: 1, allowed: 1 },
      { date: "2026-07-03", blocked: 0, allowed: 0 },
    ]);
  });
});
