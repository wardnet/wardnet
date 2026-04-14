import { Badge } from "@/components/core/ui/badge";
import type { LogEntry } from "@wardnet/js";

function levelVariant(level: string) {
  switch (level.toUpperCase()) {
    case "ERROR":
      return "destructive" as const;
    case "WARN":
      return "outline" as const;
    default:
      return "secondary" as const;
  }
}

function levelColor(level: string): string {
  switch (level.toUpperCase()) {
    case "ERROR":
      return "text-red-600 dark:text-red-400";
    case "WARN":
      return "text-yellow-600 dark:text-yellow-400";
    default:
      return "text-muted-foreground";
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
  if (entry.message && entry.message !== "response" && entry.message !== "request") {
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
    if (["method", "path", "status", "latency_ms", "name"].includes(k)) continue;
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

/** Scrollable log viewer displaying structured log entries. */
export function LogViewer({ entries, connected, skipped, maxHeight = "24rem" }: LogViewerProps) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span
            className={`inline-block size-2 rounded-full ${connected ? "bg-green-500" : "bg-red-500"}`}
          />
          <span className="text-xs text-muted-foreground">
            {connected ? "Streaming" : "Disconnected"}
          </span>
        </div>
        {skipped > 0 && (
          <span className="text-xs text-yellow-600 dark:text-yellow-400">
            {skipped} entries skipped (buffer lag)
          </span>
        )}
      </div>

      <div
        className="overflow-y-auto rounded-lg border border-border bg-muted/30 font-mono text-xs"
        style={{ maxHeight }}
      >
        {entries.length === 0 ? (
          <p className="p-4 text-center text-muted-foreground">
            {connected ? "Waiting for log entries..." : "Not connected"}
          </p>
        ) : (
          <div className="divide-y divide-border/50">
            {entries.map((entry, i) => (
              <div key={i} className={`flex gap-3 px-3 py-1.5 ${levelColor(entry.level)}`}>
                <span className="shrink-0 text-muted-foreground/60">
                  {formatTimestamp(entry.timestamp)}
                </span>
                <Badge variant={levelVariant(entry.level)} className="h-5 shrink-0 text-[10px]">
                  {entry.level}
                </Badge>
                <span className="min-w-0 break-all">{formatMessage(entry)}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
