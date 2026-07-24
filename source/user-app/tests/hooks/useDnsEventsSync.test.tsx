import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { IDBFactory } from "fake-indexeddb";

import {
  openDb,
  DAILY_STORE,
  EVENTS_STORE,
  type DailyStat,
  type DnsEventItem,
} from "../../src/lib/dnsDb";
import { getAllRows, putDaily } from "../helpers/idb";
import { useDnsEventsSync } from "../../src/hooks/useDnsEventsSync";

/** Minimal controllable EventSource stand-in. */
class FakeEventSource {
  static instances: FakeEventSource[] = [];
  url: string;
  onmessage: ((e: MessageEvent<string>) => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;
  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }
  close() {
    this.closed = true;
  }
  emit(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) } as MessageEvent<string>);
  }
  emitRaw(data: string) {
    this.onmessage?.({ data } as MessageEvent<string>);
  }
}

function evData(id: number, status = "forwarded"): DnsEventItem {
  return {
    id,
    domain: `d${id}.com`,
    status,
    captured_at: "2026-07-02T12:00:00Z",
  };
}

async function countEvents(): Promise<number> {
  const db = await openDb();
  const n = await new Promise<number>((resolve, reject) => {
    const req = db
      .transaction(EVENTS_STORE, "readonly")
      .objectStore(EVENTS_STORE)
      .count();
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
  db.close();
  return n;
}

async function dailyDates(): Promise<string[]> {
  const db = await openDb();
  const rows = await getAllRows<DailyStat>(db, DAILY_STORE);
  db.close();
  return rows.map((r) => r.date);
}

async function seedStaleDaily(): Promise<void> {
  const db = await openDb();
  await putDaily(db, {
    date: "2000-01-01",
    domain: "stale.com",
    blocked: 1,
    allowed: 0,
  });
  db.close();
}

let fetchSpy: ReturnType<typeof vi.fn>;

beforeEach(() => {
  globalThis.indexedDB = new IDBFactory();
  FakeEventSource.instances = [];
  vi.stubGlobal("EventSource", FakeEventSource);
  fetchSpy = vi.fn().mockResolvedValue({ ok: true } as Response);
  vi.stubGlobal("fetch", fetchSpy);
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("useDnsEventsSync", () => {
  it("opens the SSE stream to the daemon", async () => {
    renderHook(() => useDnsEventsSync());
    await vi.waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    expect(FakeEventSource.instances[0].url).toBe(
      "/api/devices/me/dns-events/stream",
    );
  });

  it("persists a received event and notifies", async () => {
    renderHook(() => useDnsEventsSync());
    await vi.waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const es = FakeEventSource.instances[0];

    await act(async () => {
      es.emit(evData(1, "blocked"));
      await Promise.resolve();
    });

    await vi.waitFor(async () => expect(await countEvents()).toBe(1));
  });

  it("ignores malformed JSON without throwing", async () => {
    renderHook(() => useDnsEventsSync());
    await vi.waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const es = FakeEventSource.instances[0];
    act(() => {
      es.emitRaw("not-json");
      es.onerror?.();
    });
    expect(await countEvents()).toBe(0);
  });

  it("acks the batch after ACK_BATCH_SIZE events", async () => {
    renderHook(() => useDnsEventsSync());
    await vi.waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const es = FakeEventSource.instances[0];

    await act(async () => {
      for (let i = 1; i <= 50; i += 1) {
        es.emit(evData(i));
        // Let the applyEvent promise chain settle for each.
        await Promise.resolve();
        await Promise.resolve();
      }
    });

    await vi.waitFor(() => {
      const ackCall = fetchSpy.mock.calls.find(
        (c) => c[0] === "/api/devices/me/dns-events/ack",
      );
      expect(ackCall).toBeTruthy();
    });
    const ackCall = fetchSpy.mock.calls.find(
      (c) => c[0] === "/api/devices/me/dns-events/ack",
    )!;
    expect(JSON.parse((ackCall[1] as RequestInit).body as string)).toEqual({
      up_to_id: 50,
    });
  });

  it("prunes both the events ring and the daily store on the ack tick", async () => {
    // An out-of-window aggregate row seeded before the hook mounts.
    await seedStaleDaily();

    renderHook(() => useDnsEventsSync());
    await vi.waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const es = FakeEventSource.instances[0];

    // Emit a full batch of in-window events (today) to trigger the ack flush,
    // which also runs pruneEvents + pruneDaily.
    const now = new Date().toISOString();
    await act(async () => {
      for (let i = 1; i <= 50; i += 1) {
        es.onmessage?.({
          data: JSON.stringify({
            id: i,
            domain: `d${i}.com`,
            status: "forwarded",
            captured_at: now,
          }),
        } as MessageEvent<string>);
        // Let each applyEvent promise chain settle.
        await Promise.resolve();
        await Promise.resolve();
      }
    });

    // The stale row is pruned; today's freshly aggregated rows survive.
    await vi.waitFor(async () => {
      const dates = await dailyDates();
      expect(dates).not.toContain("2000-01-01");
      expect(dates.length).toBeGreaterThan(0);
    });
    // The event ring is untouched below its cap — pruneEvents behavior intact.
    expect(await countEvents()).toBe(50);
  });

  it("flushes a pending ack on unmount", async () => {
    const { unmount } = renderHook(() => useDnsEventsSync());
    await vi.waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const es = FakeEventSource.instances[0];

    es.emit(evData(7));
    // Wait until the event is durably persisted (⇒ pendingAckId is set).
    await vi.waitFor(async () => expect(await countEvents()).toBe(1));

    unmount();

    await vi.waitFor(() => {
      expect(
        fetchSpy.mock.calls.some(
          (c) => c[0] === "/api/devices/me/dns-events/ack",
        ),
      ).toBe(true);
    });
    expect(es.closed).toBe(true);
  });
});
