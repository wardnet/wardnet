import { useEffect, useRef } from "react";

const DB_NAME = "wardnet-dns-events";
const STORE_NAME = "events";
const ACK_BATCH_SIZE = 50;
const ACK_INTERVAL_MS = 5_000;
const STREAM_PATH = "/api/devices/me/dns-events/stream";
const ACK_PATH = "/api/devices/me/dns-events/ack";

interface DnsEventItem {
  id: number;
  domain: string;
  status: string;
  captured_at: string;
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 1);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME, { keyPath: "id" });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

function writeEvent(db: IDBDatabase, item: DnsEventItem): void {
  const tx = db.transaction(STORE_NAME, "readwrite");
  tx.objectStore(STORE_NAME).put(item);
}

async function ackUpTo(upToId: number): Promise<void> {
  await fetch(ACK_PATH, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ up_to_id: upToId }),
  });
}

/**
 * Opens an SSE stream from the daemon and persists each captured DNS event
 * into IndexedDB ("wardnet-dns-events"/"events"). After every 50 events or
 * 5 seconds (whichever comes first) the highest received ID is acked so the
 * daemon can delete the rows.
 *
 * The browser manages `Last-Event-ID` automatically, so reconnects resume
 * from the last acked cursor without any extra code here.
 *
 * Mount unconditionally — the stream is a no-op when there are no pending
 * events, and the hook tears itself down cleanly on unmount.
 */
export function useDnsEventsSync(): void {
  const pendingAckId = useRef<number | null>(null);
  const pendingCount = useRef(0);

  useEffect(() => {
    let db: IDBDatabase | null = null;
    let ackTimer: ReturnType<typeof setTimeout> | null = null;
    let es: EventSource | null = null;
    let cancelled = false;

    function flushAck() {
      if (ackTimer !== null) {
        clearTimeout(ackTimer);
        ackTimer = null;
      }
      if (pendingAckId.current !== null) {
        const id = pendingAckId.current;
        pendingAckId.current = null;
        pendingCount.current = 0;
        ackUpTo(id).catch(() => {
          // Best-effort; server will re-deliver on next connect.
        });
      }
    }

    function scheduleAck() {
      if (ackTimer === null) {
        ackTimer = setTimeout(flushAck, ACK_INTERVAL_MS);
      }
    }

    async function start() {
      db = await openDb();
      if (cancelled) {
        db.close();
        return;
      }

      es = new EventSource(STREAM_PATH);

      es.onmessage = (e: MessageEvent<string>) => {
        let item: DnsEventItem;
        try {
          item = JSON.parse(e.data) as DnsEventItem;
        } catch {
          return;
        }

        if (db) {
          writeEvent(db, item);
        }

        pendingAckId.current = item.id;
        pendingCount.current += 1;

        if (pendingCount.current >= ACK_BATCH_SIZE) {
          flushAck();
        } else {
          scheduleAck();
        }
      };

      es.onerror = () => {
        // Browser will retry with Last-Event-ID automatically.
      };
    }

    start().catch(() => {
      // IndexedDB not available (private browsing / unsupported). Degrade
      // gracefully — the daemon keeps rows until acked so nothing is lost.
    });

    return () => {
      cancelled = true;
      es?.close();
      if (ackTimer !== null) clearTimeout(ackTimer);
      flushAck();
      db?.close();
    };
  }, []);
}
