import type { LogEntry } from "@wardnet/js";

/** Map a LogEntry level to the `.logrow` CSS modifier suffix. */
function levelModifier(level: string): "err" | "warn" | "info" {
  switch (level.toUpperCase()) {
    case "ERROR":
      return "err";
    case "WARN":
      return "warn";
    default:
      return "info";
  }
}

function formatTimestamp(ts: string): string {
  if (!ts) return "";
  const date = new Date(ts);
  const hms = date.toLocaleTimeString([], { hour12: false });
  const ms = String(date.getMilliseconds()).padStart(3, "0");
  return `${hms}.${ms}`;
}

/** Build a display string from the entry. Uses message + key span/event fields. */
function formatMessage(entry: LogEntry): string {
  const parts: string[] = [];

  // If the message is generic (e.g. "response"), use span context instead.
  if (
    entry.message &&
    entry.message !== "response" &&
    entry.message !== "request"
  ) {
    parts.push(entry.message);
  }

  // Add HTTP request context from span fields.
  const span = entry.span ?? {};
  if (span.method && span.path) {
    const status = span.status ? ` ${span.status}` : "";
    const latency = span.latency_ms ? ` (${span.latency_ms}ms)` : "";
    parts.push(`${span.method} ${span.path}${status}${latency}`);
  }

  // Collect structured fields, deduplicating fields that appear in both
  // event fields and span with the same name and value.
  const fields = entry.fields ?? {};
  const seen = new Set<string>();
  const fieldParts: string[] = [];

  for (const [k, v] of Object.entries(fields)) {
    if (k === "message") continue;
    const key = `${k}=${v}`;
    if (!seen.has(key)) {
      seen.add(key);
      fieldParts.push(key);
    }
  }
  for (const [k, v] of Object.entries(span)) {
    // Skip span fields already shown as HTTP context above.
    if (["method", "path", "status", "latency_ms", "name"].includes(k))
      continue;
    const key = `${k}=${v}`;
    if (!seen.has(key)) {
      seen.add(key);
      fieldParts.push(key);
    }
  }

  if (fieldParts.length > 0) {
    parts.push(`[${fieldParts.join(" · ")}]`);
  }

  return parts.join(" ") || entry.message || entry.target;
}

interface LogViewerProps {
  entries: LogEntry[];
  connected: boolean;
  skipped: number;
  maxHeight?: string;
}

/** Scrollable log viewer displaying structured log entries.
 *
 * Renders the `.logs` element directly so a parent CardContent's
 * `:has(> .logs)` flush rule applies — matching the Recent errors
 * card. A buffer-lag warning (if any) is rendered as a sticky
 * `.logrow` at the top of the scroll area so it doesn't push the
 * card out of its flush layout. */
export function LogViewer({
  entries,
  connected,
  skipped,
  maxHeight = "24rem",
}: LogViewerProps) {
  return (
    <div className="logs" style={{ maxHeight }}>
      {skipped > 0 && (
        <div className="logrow is-warn">
          <div className="t">—</div>
          <div className="l">SKIP</div>
          <div className="m">{skipped} entries skipped (buffer lag)</div>
        </div>
      )}
      {entries.length === 0 ? (
        <p className="logrow is-info">
          <span className="m">
            {connected ? "Waiting for log entries…" : "Not connected"}
          </span>
        </p>
      ) : (
        entries.map((entry, i) => (
          <div key={i} className={`logrow is-${levelModifier(entry.level)}`}>
            <div className="t">{formatTimestamp(entry.timestamp)}</div>
            <div className="l">{entry.level.toUpperCase()}</div>
            <div className="m">{formatMessage(entry)}</div>
          </div>
        ))
      )}
    </div>
  );
}
