import type { Anomaly } from "@wardnet/js";
import { Card, CardContent, CardHeader, CardTitle, Text } from "@wardnet/web";
import { anomalySubjectLink } from "@wardnet/web";
import { Link } from "react-router";

import { ADMIN_SITE_ANOMALY_ROUTES } from "../../lib/anomalyRoutes";

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

/** Map an anomaly severity to the `.logrow` colour modifier. */
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

interface AnomaliesCardProps {
  anomalies: Anomaly[];
}

/** Dashboard card showing the conditions currently open on the box. */
export function AnomaliesCard({ anomalies }: AnomaliesCardProps) {
  // Newest first, ordered by when each condition was first raised (not
  // arrival order) so the column reads chronologically.
  const rows = [...anomalies].sort(
    (a, b) => toMillis(b.opened_at) - toMillis(a.opened_at),
  );

  return (
    // `dashboard-anomalies` lets the visual-regression suite mask this card
    // (#628): live rows carry per-run timestamps and messages.
    <Card data-testid="dashboard-anomalies">
      <CardHeader>
        <CardTitle>Anomalies</CardTitle>
      </CardHeader>
      <CardContent>
        {rows.length === 0 ? (
          <Text as="p" size="sm" className="py-4 text-center text-ink-3">
            Nothing wrong right now
          </Text>
        ) : (
          <div className="logs">
            {rows.map((anomaly) => {
              const link = anomalySubjectLink(
                anomaly,
                ADMIN_SITE_ANOMALY_ROUTES,
              );
              return (
                <div
                  key={anomaly.id}
                  className={`logrow ${severityClass(anomaly.severity)}`}
                  data-testid="anomaly-row"
                >
                  <div className="t">{formatTimestamp(anomaly.opened_at)}</div>
                  <div className="l">{severityLabel(anomaly.severity)}</div>
                  <div className="m">
                    <span className="flex items-start gap-1.5">
                      <span>{anomaly.message}</span>
                      {link && (
                        <Link
                          to={link.href}
                          aria-label={`${link.label}: ${anomaly.message}`}
                          data-testid="anomaly-subject-link"
                          // Spelled out rather than a bare glyph: an arrow on
                          // its own reads as decoration, says nothing about
                          // where it goes, and is a small target. The label is
                          // already resolved per anomaly type, and showing it
                          // keeps the visible text inside the accessible name
                          // (WCAG 2.5.3) rather than diverging from it.
                          className="shrink-0 whitespace-nowrap text-accent hover:underline"
                        >
                          {link.label}
                          <span aria-hidden className="select-none">
                            {" ↗"}
                          </span>
                        </Link>
                      )}
                    </span>
                    {anomaly.hint && (
                      <span className="mt-1 flex gap-1.5 text-ink-3">
                        <span aria-hidden className="select-none">
                          →
                        </span>
                        <span>{anomaly.hint}</span>
                      </span>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
