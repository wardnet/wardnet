// Read helpers over the device-local DNS stats database. All reads come from
// IndexedDB — the Stats page never queries the daemon.

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

export async function getDayHeadline(date: string): Promise<DayHeadline> {
  const db = await openDb();
  try {
    const rows = await rowsForDate(db, date);
    let blocked = 0;
    let allowed = 0;
    for (const r of rows) {
      blocked += r.blocked;
      allowed += r.allowed;
    }
    return { total: blocked + allowed, blocked, allowed };
  } finally {
    db.close();
  }
}

async function topByDate(
  date: string,
  pick: (r: DailyStat) => number,
  n: number,
): Promise<DomainCount[]> {
  const db = await openDb();
  try {
    const rows = await rowsForDate(db, date);
    return rows
      .map((r) => ({ domain: r.domain, count: pick(r) }))
      .filter((d) => d.count > 0)
      .sort((a, b) => b.count - a.count)
      .slice(0, n);
  } finally {
    db.close();
  }
}

export function getTopBlocked(date: string, n = 10): Promise<DomainCount[]> {
  return topByDate(date, (r) => r.blocked, n);
}

export function getTopQueried(date: string, n = 10): Promise<DomainCount[]> {
  return topByDate(date, (r) => r.blocked + r.allowed, n);
}

/** Per-day blocked/allowed totals for the given local dates (oldest first). */
export async function getTrend(dates: string[]): Promise<TrendDay[]> {
  const db = await openDb();
  try {
    const out: TrendDay[] = [];
    for (const date of dates) {
      const rows = await rowsForDate(db, date);
      let blocked = 0;
      let allowed = 0;
      for (const r of rows) {
        blocked += r.blocked;
        allowed += r.allowed;
      }
      out.push({ date, blocked, allowed });
    }
    return out;
  } finally {
    db.close();
  }
}

/** The `n` most recent raw events, newest first. */
export async function getRecentActivity(n = 20): Promise<DnsEventItem[]> {
  const db = await openDb();
  try {
    return await new Promise<DnsEventItem[]>((resolve, reject) => {
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
  } finally {
    db.close();
  }
}

/** True if any aggregated data exists (used to distinguish empty states). */
export async function hasAnyData(): Promise<boolean> {
  const db = await openDb();
  try {
    return await new Promise<boolean>((resolve, reject) => {
      const tx = db.transaction(DAILY_STORE, "readonly");
      const req = tx.objectStore(DAILY_STORE).count();
      req.onsuccess = () => resolve(req.result > 0);
      req.onerror = () => reject(req.error);
    });
  } finally {
    db.close();
  }
}
