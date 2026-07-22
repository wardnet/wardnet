// Shared IndexedDB test helpers for the DNS-stats stores. Kept out of the
// React-oriented test-utils.tsx so both the dnsDb and useDnsEventsSync suites
// can reuse the same open-transaction plumbing instead of re-declaring it.

import { DAILY_STORE, type DailyStat } from "../../src/lib/dnsDb";

/** Read every row from an object store. */
export function getAllRows<T>(db: IDBDatabase, store: string): Promise<T[]> {
  return new Promise((resolve, reject) => {
    const req = db.transaction(store, "readonly").objectStore(store).getAll();
    req.onsuccess = () => resolve(req.result as T[]);
    req.onerror = () => reject(req.error);
  });
}

/** Insert a single pre-aggregated daily row. */
export function putDaily(db: IDBDatabase, row: DailyStat): Promise<void> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(DAILY_STORE, "readwrite");
    tx.objectStore(DAILY_STORE).put(row);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}
