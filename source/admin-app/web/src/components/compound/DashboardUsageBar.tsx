/** Horizontal usage bar with color thresholds (green → yellow → red). */
export function DashboardUsageBar({ value }: { value: number }) {
  const clamped = Math.min(100, Math.max(0, value));
  const fill = clamped > 80 ? "var(--danger)" : clamped > 50 ? "var(--warn)" : "var(--accent)";

  return (
    <div className="bar mt-2">
      <span style={{ width: `${clamped}%`, background: fill }} />
    </div>
  );
}
