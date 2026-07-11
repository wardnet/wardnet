import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IDBFactory } from "fake-indexeddb";

import {
  DAILY_STORE,
  EVENTS_STORE,
  META_STORE,
  applyEvent,
  localDate,
  notifyStatsChanged,
  openDb,
  pruneEvents,
  recentDates,
  subscribeStats,
  todayLocal,
  type DailyStat,
  type DnsEventItem,
} from "../../src/lib/dnsDb";

function ev(overrides: Partial<DnsEventItem> = {}): DnsEventItem {
  return {
    id: 1,
    domain: "example.com",
    status: "forwarded",
    captured_at: "2026-07-02T12:00:00Z",
    ...overrides,
  };
}

function getAll<T>(db: IDBDatabase, store: string): Promise<T[]> {
  return new Promise((resolve, reject) => {
    const req = db.transaction(store, "readonly").objectStore(store).getAll();
    req.onsuccess = () => resolve(req.result as T[]);
    req.onerror = () => reject(req.error);
  });
}

function getMeta(db: IDBDatabase, key: string): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const req = db
      .transaction(META_STORE, "readonly")
      .objectStore(META_STORE)
      .get(key);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

beforeEach(() => {
  // Isolate every test with a brand-new in-memory IndexedDB.
  globalThis.indexedDB = new IDBFactory();
});

describe("localDate / todayLocal / recentDates", () => {
  it("formats an RFC3339 timestamp as a local YYYY-MM-DD date", () => {
    // Use a mid-day UTC time so the local date matches regardless of TZ offset.
    expect(localDate("2026-07-02T12:00:00Z")).toBe("2026-07-02");
  });

  it("todayLocal matches localDate(now)", () => {
    expect(todayLocal()).toBe(localDate(new Date().toISOString()));
  });

  it("recentDates returns `count` dates ending today, oldest first", () => {
    const dates = recentDates(7);
    expect(dates).toHaveLength(7);
    expect(dates[6]).toBe(todayLocal());
    // Strictly ascending.
    for (let i = 1; i < dates.length; i += 1) {
      // eslint-disable-next-line security/detect-object-injection -- loop index over the local recentDates(7) array in a test assertion
      expect(dates[i] > dates[i - 1]).toBe(true);
    }
  });
});

describe("openDb", () => {
  it("creates the three object stores", async () => {
    const db = await openDb();
    expect(db.objectStoreNames.contains(EVENTS_STORE)).toBe(true);
    expect(db.objectStoreNames.contains(DAILY_STORE)).toBe(true);
    expect(db.objectStoreNames.contains(META_STORE)).toBe(true);
    db.close();
  });

  it("rejects when indexedDB.open errors", async () => {
    const fakeReq: {
      onupgradeneeded: (() => void) | null;
      onsuccess: (() => void) | null;
      onerror: (() => void) | null;
      error: Error;
      result: unknown;
    } = {
      onupgradeneeded: null,
      onsuccess: null,
      onerror: null,
      error: new Error("boom"),
      result: null,
    };
    const spy = vi
      .spyOn(globalThis.indexedDB, "open")
      .mockReturnValue(fakeReq as unknown as IDBOpenDBRequest);
    const promise = openDb();
    fakeReq.onerror?.();
    await expect(promise).rejects.toThrow("boom");
    spy.mockRestore();
  });
});

describe("applyEvent", () => {
  it("stores the raw event and increments the daily counter", async () => {
    const db = await openDb();
    await applyEvent(db, ev({ id: 1, status: "blocked" }));
    await applyEvent(db, ev({ id: 2, status: "forwarded" }));

    const events = await getAll<DnsEventItem>(db, EVENTS_STORE);
    expect(events).toHaveLength(2);

    const daily = await getAll<DailyStat>(db, DAILY_STORE);
    expect(daily).toHaveLength(1);
    expect(daily[0]).toMatchObject({
      date: "2026-07-02",
      domain: "example.com",
      blocked: 1,
      allowed: 1,
    });

    const cursor = (await getMeta(db, "lastAggregatedId")) as {
      value: number;
    };
    expect(cursor.value).toBe(2);
    db.close();
  });

  it("counts only status 'blocked' as blocked; everything else as allowed", async () => {
    const db = await openDb();
    await applyEvent(db, ev({ id: 1, status: "blocked" }));
    await applyEvent(db, ev({ id: 2, status: "blocked_skipped" }));
    await applyEvent(db, ev({ id: 3, status: "cache_hit" }));

    const [row] = await getAll<DailyStat>(db, DAILY_STORE);
    expect(row.blocked).toBe(1);
    expect(row.allowed).toBe(2);
    db.close();
  });

  it("is idempotent for re-delivered events (id <= cursor)", async () => {
    const db = await openDb();
    await applyEvent(db, ev({ id: 5, status: "blocked" }));
    // Re-deliver a lower and equal id — both should be ignored.
    await applyEvent(db, ev({ id: 5, status: "blocked" }));
    await applyEvent(db, ev({ id: 3, status: "blocked" }));

    const [row] = await getAll<DailyStat>(db, DAILY_STORE);
    expect(row.blocked).toBe(1);
    const events = await getAll<DnsEventItem>(db, EVENTS_STORE);
    expect(events).toHaveLength(1);
    db.close();
  });

  it("buckets events into per-domain daily rows", async () => {
    const db = await openDb();
    await applyEvent(db, ev({ id: 1, domain: "a.com", status: "blocked" }));
    await applyEvent(db, ev({ id: 2, domain: "b.com", status: "forwarded" }));
    const daily = await getAll<DailyStat>(db, DAILY_STORE);
    expect(daily).toHaveLength(2);
    db.close();
  });

  it("rejects when the transaction aborts", async () => {
    const db = await openDb();
    // Force an abort by mocking transaction to a stub whose onabort fires.
    const realTx = db.transaction.bind(db);
    const tx = realTx([EVENTS_STORE, DAILY_STORE, META_STORE], "readwrite");
    const spy = vi.spyOn(db, "transaction").mockReturnValue(tx);
    const promise = applyEvent(db, ev({ id: 1 }));
    tx.abort();
    let rejected = false;
    await promise.catch(() => {
      rejected = true;
    });
    expect(rejected).toBe(true);
    spy.mockRestore();
    db.close();
  });
});

describe("pruneEvents", () => {
  it("does nothing when at or below the ring cap", async () => {
    const db = await openDb();
    for (let i = 1; i <= 5; i += 1) {
      await applyEvent(db, ev({ id: i }));
    }
    await pruneEvents(db);
    const events = await getAll<DnsEventItem>(db, EVENTS_STORE);
    expect(events).toHaveLength(5);
    db.close();
  });

  it("trims the ring to the newest EVENTS_RING_CAP entries", async () => {
    const db = await openDb();
    // 502 events → 2 over the 500 cap → the two oldest ids dropped.
    for (let i = 1; i <= 502; i += 1) {
      await applyEvent(db, ev({ id: i }));
    }
    await pruneEvents(db);
    const events = await getAll<DnsEventItem>(db, EVENTS_STORE);
    expect(events).toHaveLength(500);
    const ids = events.map((e) => e.id).sort((a, b) => a - b);
    expect(ids[0]).toBe(3);
    db.close();
  });
});

describe("stats pub/sub", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("notifies subscribers and stops after unsubscribe", () => {
    const cb = vi.fn();
    const unsubscribe = subscribeStats(cb);
    notifyStatsChanged();
    expect(cb).toHaveBeenCalledTimes(1);
    unsubscribe();
    notifyStatsChanged();
    expect(cb).toHaveBeenCalledTimes(1);
  });
});
