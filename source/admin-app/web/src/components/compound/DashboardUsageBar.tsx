/** Horizontal usage bar with color thresholds (green → yellow → red). */
export function DashboardUsageBar({ value }: { value: number }) {
  const clamped = Math.min(100, Math.max(0, value));
  const color = clamped > 80 ? "bg-destructive" : clamped > 50 ? "bg-yellow-500" : "bg-primary";

  return (
    <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-muted">
      <div className={`h-full rounded-full ${color}`} style={{ width: `${clamped}%` }} />
    </div>
  );
}
