/** Description slot used in progress toasts — a thin progress bar plus the
 *  numeric percentage. Fed by the polled job status. */
export function JobProgressDescription({ percentage }: { percentage: number }) {
  const pct = Math.max(0, Math.min(100, percentage));
  return (
    <div className="mt-1 flex items-center gap-2">
      <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
        <div
          className="h-full bg-primary transition-all duration-300"
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="text-xs tabular-nums text-muted-foreground">{pct}%</span>
    </div>
  );
}
