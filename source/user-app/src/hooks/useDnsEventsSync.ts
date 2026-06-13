import { useEffect, useRef } from "react";

import {
  applyEvent,
  type DnsEventItem,
  notifyStatsChanged,
  openDb,
  pruneEvents,
} from "../lib/dnsDb.js";

const ACK_BATCH_SIZE = 50;
const ACK_INTERVAL_MS = 5_000;
const STREAM_PATH = "/api/devices/me/dns-events/stream";
const ACK_PATH = "/api/devices/me/dns-events/ack";

async function ackUpTo(upToId: number): Promise<void> {
  await fetch(ACK_PATH, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ up_to_id: upToId }),
  });
}

/**
 * Opens an SSE stream from the daemon and persists each captured DNS event
 * into the device-local IndexedDB store (see `lib/dnsDb`). Each event is stored
 * raw (for the activity feed) and folded into the per-day, per-domain
 * aggregate that the Stats page reads. After every 50 events or 5 seconds the
 * highest received ID is acked so the daemon can delete the rows, and the raw
 * event ring is pruned.
 *
 * The browser manages `Last-Event-ID` automatically, so reconnects resume
 * from the last acked cursor; the `lastAggregatedId` cursor in IndexedDB keeps
 * re-delivered rows from being double-counted.
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
        if (db) {
          pruneEvents(db).catch(() => {
            // Pruning is best-effort housekeeping.
          });
        }
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
          applyEvent(db, item)
            .then(() => {
              pendingAckId.current = item.id;
              pendingCount.current += 1;
              notifyStatsChanged();
              if (pendingCount.current >= ACK_BATCH_SIZE) {
                flushAck();
              } else {
                scheduleAck();
              }
            })
            .catch(() => {
              // On write failure, don't advance cursor — daemon re-delivers.
            });
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
