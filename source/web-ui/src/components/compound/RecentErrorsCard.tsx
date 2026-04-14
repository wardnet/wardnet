import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Badge } from "@/components/core/ui/badge";
import type { RecentError } from "@/hooks/useSystemStatus";

function formatTimestamp(ts: string): string {
  if (!ts) return "";
  const date = new Date(ts);
  const hms = date.toLocaleTimeString([], { hour12: false });
  const ms = String(date.getMilliseconds()).padStart(3, "0");
  return `${hms}.${ms}`;
}

interface RecentErrorsCardProps {
  errors: RecentError[];
}

/** Dashboard card showing the most recent warnings and errors. */
export function RecentErrorsCard({ errors }: RecentErrorsCardProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm font-semibold">Recent Errors</CardTitle>
      </CardHeader>
      <CardContent>
        {errors.length === 0 ? (
          <p className="py-4 text-center text-sm text-muted-foreground">No recent errors</p>
        ) : (
          <div className="flex max-h-60 flex-col gap-1 overflow-y-auto font-mono text-xs">
            {[...errors].reverse().map((err, i) => (
              <div
                key={i}
                className={`flex gap-2 rounded px-2 py-1 ${
                  err.level === "ERROR"
                    ? "bg-red-50 text-red-700 dark:bg-red-950/50 dark:text-red-300"
                    : "bg-yellow-50 text-yellow-700 dark:bg-yellow-950/50 dark:text-yellow-300"
                }`}
              >
                <span className="shrink-0 text-muted-foreground/60">
                  {formatTimestamp(err.timestamp)}
                </span>
                <Badge
                  variant={err.level === "ERROR" ? "destructive" : "outline"}
                  className="h-5 shrink-0 text-[10px]"
                >
                  {err.level}
                </Badge>
                <span className="min-w-0 break-all">{err.message}</span>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
