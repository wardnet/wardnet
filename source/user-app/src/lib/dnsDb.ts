// IndexedDB layer for device-local DNS stats.
//
// One database, three stores:
//   - `events` — a bounded ring of the most recent raw events (the activity
//     feed). Pruned to EVENTS_RING_CAP newest by id.
//   - `daily`  — pre-aggregated per-day, per-domain counts keyed [date, domain].
//     This is what the Stats page queries, so day/history reads stay O(rows
//     for that day) instead of scanning every raw event.
//   - `meta`   — a single `lastAggregatedId` cursor so re-delivered events
//     (the SSE stream replays unacked rows on reconnect) are never counted
//     twice in `daily`.
//
// "Blocked" semantics: only status === "blocked" counts as blocked. Everything
// else (forwarded, cache_hit, rewritten, recursive, authoritative,
// blocked_skipped, …) counts as allowed — blocked_skipped means the query was
// actually answered, so counting it as blocked would be dishonest.

export const DB_NAME = "wardnet-dns-events";
export const DB_VERSION = 2;
export const EVENTS_STORE = "events";
export const DAILY_STORE = "daily";
export const META_STORE = "meta";

const EVENTS_RING_CAP = 500;
const CURSOR_KEY = "lastAggregatedId";

export interface DnsEventItem {
  id: number;
  domain: string;
  status: string;
  captured_at: string;
}

export interface DailyStat {
  date: string;
  domain: string;
  blocked: number;
  allowed: number;
}

/** Local-time ISO date (YYYY-MM-DD) for an RFC3339 timestamp. */
export function localDate(iso: string): string {
  const d = new Date(iso);
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** Today's local date (YYYY-MM-DD). */
export function todayLocal(): string {
  return localDate(new Date().toISOString());
}

/** The last `count` local dates ending today, oldest first. */
export function recentDates(count: number): string[] {
  const out: string[] = [];
  const now = new Date();
  for (let i = count - 1; i >= 0; i -= 1) {
    const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i);
    out.push(localDate(d.toISOString()));
  }
  return out;
}

function isBlocked(status: string): boolean {
  return status === "blocked";
}

export function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(EVENTS_STORE)) {
        db.createObjectStore(EVENTS_STORE, { keyPath: "id" });
      }
      if (!db.objectStoreNames.contains(DAILY_STORE)) {
        db.createObjectStore(DAILY_STORE, { keyPath: ["date", "domain"] });
      }
      if (!db.objectStoreNames.contains(META_STORE)) {
        db.createObjectStore(META_STORE, { keyPath: "key" });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

/**
 * Apply a single captured event: store the raw event, increment the
 * [date, domain] daily counter, and advance the aggregation cursor — all in
 * one transaction. Events with `id <= lastAggregatedId` are ignored (idempotent
 * re-delivery). IndexedDB serialises overlapping readwrite transactions, so
 * concurrent calls to this function never double-count.
 *
 * Resolves once the transaction commits.
 */
export function applyEvent(db: IDBDatabase, item: DnsEventItem): Promise<void> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction([EVENTS_STORE, DAILY_STORE, META_STORE], "readwrite");
    const events = tx.objectStore(EVENTS_STORE);
    const daily = tx.objectStore(DAILY_STORE);
    const meta = tx.objectStore(META_STORE);

    const cursorReq = meta.get(CURSOR_KEY);
    cursorReq.onsuccess = () => {
      const cursor = (cursorReq.result?.value as number | undefined) ?? 0;
      if (item.id <= cursor) {
        return; // already aggregated — skip, tx commits as a no-op
      }
      events.put(item);

      const date = localDate(item.captured_at);
      const key: [string, string] = [date, item.domain];
      const getReq = daily.get(key);
      getReq.onsuccess = () => {
        const prev =
          (getReq.result as DailyStat | undefined) ??
          ({ date, domain: item.domain, blocked: 0, allowed: 0 } satisfies DailyStat);
        if (isBlocked(item.status)) {
          prev.blocked += 1;
        } else {
          prev.allowed += 1;
        }
        daily.put(prev);
      };

      meta.put({ key: CURSOR_KEY, value: item.id });
    };

    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error ?? new Error("transaction aborted"));
  });
}

/** Trim the raw `events` ring to its newest EVENTS_RING_CAP entries. */
export function pruneEvents(db: IDBDatabase): Promise<void> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(EVENTS_STORE, "readwrite");
    const store = tx.objectStore(EVENTS_STORE);
    const countReq = store.count();
    countReq.onsuccess = () => {
      const excess = countReq.result - EVENTS_RING_CAP;
      if (excess <= 0) {
        return;
      }
      let removed = 0;
      // Oldest ids first (ascending) — delete the excess from the front.
      store.openCursor().onsuccess = (e) => {
        const cur = (e.target as IDBRequest<IDBCursorWithValue | null>).result;
        if (cur && removed < excess) {
          cur.delete();
          removed += 1;
          cur.continue();
        }
      };
    };
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error ?? new Error("transaction aborted"));
  });
}

// --- stats-change pub/sub ---------------------------------------------------
// The sync hook calls notifyStatsChanged() after writing events; the Stats page
// subscribes to re-query. Kept here so both sides share one channel.

const listeners = new Set<() => void>();

export function notifyStatsChanged(): void {
  for (const l of listeners) {
    l();
  }
}

export function subscribeStats(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}
