import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import { IDBFactory } from "fake-indexeddb";

import {
  applyEvent,
  notifyStatsChanged,
  openDb,
  todayLocal,
  type DnsEventItem,
} from "../../src/lib/dnsDb";
import { useDnsStats } from "../../src/hooks/useDnsStats";

function ev(overrides: Partial<DnsEventItem> = {}): DnsEventItem {
  return {
    id: 1,
    domain: "example.com",
    status: "forwarded",
    captured_at: `${todayLocal()}T12:00:00`,
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

afterEach(() => {
  vi.restoreAllMocks();
});

describe("useDnsStats", () => {
  it("starts loading, then resolves with the day's snapshot", async () => {
    await seed([ev({ id: 1, status: "blocked" })]);
    const today = todayLocal();

    const { result } = renderHook(() => useDnsStats(today));
    expect(result.current.loading).toBe(true);

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.hasData).toBe(true);
    expect(result.current.headline.blocked).toBe(1);
  });

  it("reloads (debounced) when stats change is notified", async () => {
    const today = todayLocal();
    const { result } = renderHook(() => useDnsStats(today));
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.hasData).toBe(false);

    await seed([ev({ id: 1, status: "blocked" })]);
    // Multiple notifications inside the debounce window coalesce into one reload.
    act(() => {
      notifyStatsChanged();
      notifyStatsChanged();
    });

    await waitFor(() => expect(result.current.hasData).toBe(true), {
      timeout: 2000,
    });
    expect(result.current.headline.blocked).toBe(1);
  });

  it("clears loading even when the load throws", async () => {
    vi.spyOn(globalThis.indexedDB, "open").mockImplementation(() => {
      throw new Error("no idb");
    });
    const { result } = renderHook(() => useDnsStats(todayLocal()));
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.hasData).toBe(false);
  });
});
