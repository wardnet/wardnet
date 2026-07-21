import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Text } from "@wardnet/web";
import type { SystemDiagnostic } from "@wardnet/web";

function formatTimestamp(ts: string): string {
  if (!ts) return "";
  const date = new Date(ts);
  const hms = date.toLocaleTimeString([], { hour12: false });
  const ms = String(date.getMilliseconds()).padStart(3, "0");
  return `${hms}.${ms}`;
}

/** Epoch millis for sorting; unparseable/empty timestamps sink to the bottom. */
function toMillis(ts: string): number {
  const n = Date.parse(ts);
  return Number.isNaN(n) ? 0 : n;
}

/** Map a diagnostic severity to the `.logrow` colour modifier. */
function severityClass(severity: string): string {
  if (severity === "error") return "is-err";
  if (severity === "warning") return "is-warn";
  return "is-info";
}

/** Short uppercase label shown in the level column. */
function severityLabel(severity: string): string {
  if (severity === "error") return "ERR";
  if (severity === "warning") return "WARN";
  return "INFO";
}

interface RecentErrorsCardProps {
  errors: SystemDiagnostic[];
}

/** Dashboard card showing the most recent admin-facing diagnostics. */
export function RecentErrorsCard({ errors }: RecentErrorsCardProps) {
  // Newest first, ordered by the event timestamp shown in each row (not
  // arrival order) so the column reads chronologically.
  const rows = [...errors].sort(
    (a, b) => toMillis(b.timestamp) - toMillis(a.timestamp),
  );

  return (
    // `dashboard-recent-errors` lets the visual-regression suite mask this
    // card (#628): live error rows carry per-run timestamps and messages.
    <Card data-testid="dashboard-recent-errors">
      <CardHeader>
        <CardTitle>Recent errors</CardTitle>
      </CardHeader>
      <CardContent>
        {errors.length === 0 ? (
          <Text as="p" size="sm" className="py-4 text-center text-ink-3">
            No recent errors
          </Text>
        ) : (
          <div className="logs">
            {rows.map((err, i) => (
              <div key={i} className={`logrow ${severityClass(err.severity)}`}>
                <div className="t">{formatTimestamp(err.timestamp)}</div>
                <div className="l">{severityLabel(err.severity)}</div>
                <div className="m">
                  <span>{err.message}</span>
                  {err.hint && (
                    <span className="mt-1 flex gap-1.5 text-ink-3">
                      <span aria-hidden className="select-none">
                        →
                      </span>
                      <span>{err.hint}</span>
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
