import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Text } from "@wardnet/web";
import type { RecentError } from "@wardnet/web";

function formatTimestamp(ts: string): string {
  if (!ts) return "";
  const date = new Date(ts);
  const hms = date.toLocaleTimeString([], { hour12: false });
  const ms = String(date.getMilliseconds()).padStart(3, "0");
  return `${hms}.${ms}`;
}

function levelClass(level: string): string {
  if (level === "ERROR") return "is-err";
  if (level === "WARN" || level === "WARNING") return "is-warn";
  return "is-info";
}

interface RecentErrorsCardProps {
  errors: RecentError[];
}

/** Dashboard card showing the most recent warnings and errors. */
export function RecentErrorsCard({ errors }: RecentErrorsCardProps) {
  return (
    <Card>
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
            {[...errors].reverse().map((err, i) => (
              <div key={i} className={`logrow ${levelClass(err.level)}`}>
                <div className="t">{formatTimestamp(err.timestamp)}</div>
                <div className="l">{err.level}</div>
                <div className="m">{err.message}</div>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
