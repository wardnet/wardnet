// Read helpers over the device-local DNS stats database. All reads come from
// IndexedDB — the Stats page never queries the daemon.
//
// The read helpers take an already-open `db` handle so a single refresh can
// open one connection, run every query concurrently, and close once — instead
// of opening/closing a connection per query.

import {
  DAILY_STORE,
  EVENTS_STORE,
  type DailyStat,
  type DnsEventItem,
  openDb,
} from "./dnsDb.js";

export interface DayHeadline {
  total: number;
  blocked: number;
  allowed: number;
}

export interface DomainCount {
  domain: string;
  count: number;
}

export interface TrendDay {
  date: string;
  blocked: number;
  allowed: number;
}

export interface DnsStatsSnapshot {
  headline: DayHeadline;
  topBlocked: DomainCount[];
  topQueried: DomainCount[];
  recent: DnsEventItem[];
  trend: TrendDay[];
  hasData: boolean;
}

/** All daily rows for a single local date (every domain seen that day). */
function rowsForDate(db: IDBDatabase, date: string): Promise<DailyStat[]> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(DAILY_STORE, "readonly");
    const store = tx.objectStore(DAILY_STORE);
    // Composite key [date, domain] — bound the range to a single date.
    // ￿ is the highest BMP code unit, so it sorts after any real domain.
    const range = IDBKeyRange.bound([date, ""], [date, "￿"]);
    const req = store.getAll(range);
    req.onsuccess = () => resolve((req.result as DailyStat[]) ?? []);
    req.onerror = () => reject(req.error);
  });
}

async function dayHeadline(
  db: IDBDatabase,
  date: string,
): Promise<DayHeadline> {
  const rows = await rowsForDate(db, date);
  let blocked = 0;
  let allowed = 0;
  for (const r of rows) {
    blocked += r.blocked;
    allowed += r.allowed;
  }
  return { total: blocked + allowed, blocked, allowed };
}

async function topByDate(
  db: IDBDatabase,
  date: string,
  pick: (r: DailyStat) => number,
  n: number,
): Promise<DomainCount[]> {
  const rows = await rowsForDate(db, date);
  return rows
    .map((r) => ({ domain: r.domain, count: pick(r) }))
    .filter((d) => d.count > 0)
    .sort((a, b) => b.count - a.count)
    .slice(0, n);
}

/** Per-day blocked/allowed totals for the given local dates (oldest first). */
async function trendFor(db: IDBDatabase, dates: string[]): Promise<TrendDay[]> {
  // Each date is an independent readonly range query — run them concurrently.
  return Promise.all(
    dates.map(async (date) => {
      const rows = await rowsForDate(db, date);
      let blocked = 0;
      let allowed = 0;
      for (const r of rows) {
        blocked += r.blocked;
        allowed += r.allowed;
      }
      return { date, blocked, allowed };
    }),
  );
}

/** The `n` most recent raw events, newest first. */
function recentActivity(db: IDBDatabase, n: number): Promise<DnsEventItem[]> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(EVENTS_STORE, "readonly");
    const store = tx.objectStore(EVENTS_STORE);
    const out: DnsEventItem[] = [];
    // Descending by id (newest first).
    store.openCursor(null, "prev").onsuccess = (e) => {
      const cur = (e.target as IDBRequest<IDBCursorWithValue | null>).result;
      if (cur && out.length < n) {
        out.push(cur.value as DnsEventItem);
        cur.continue();
      } else {
        resolve(out);
      }
    };
    tx.onerror = () => reject(tx.error);
  });
}

function hasAnyData(db: IDBDatabase): Promise<boolean> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(DAILY_STORE, "readonly");
    const req = tx.objectStore(DAILY_STORE).count();
    req.onsuccess = () => resolve(req.result > 0);
    req.onerror = () => reject(req.error);
  });
}

/**
 * Load the full stats snapshot for a day plus the trend window in a single
 * IndexedDB connection. Opens once, runs every read concurrently, closes once.
 */
export async function loadDnsStats(
  date: string,
  trendDates: string[],
  recentLimit = 20,
  topN = 10,
): Promise<DnsStatsSnapshot> {
  const db = await openDb();
  try {
    const [headline, topBlocked, topQueried, recent, trend, dataPresent] =
      await Promise.all([
        dayHeadline(db, date),
        topByDate(db, date, (r) => r.blocked, topN),
        topByDate(db, date, (r) => r.blocked + r.allowed, topN),
        recentActivity(db, recentLimit),
        trendFor(db, trendDates),
        hasAnyData(db),
      ]);
    return {
      headline,
      topBlocked,
      topQueried,
      recent,
      trend,
      hasData: dataPresent,
    };
  } finally {
    db.close();
  }
}
