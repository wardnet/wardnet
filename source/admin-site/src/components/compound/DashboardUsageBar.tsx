import { brand, status } from "@wardnet/styles/tokens";

/** Horizontal usage bar with color thresholds (green → yellow → red). */
export function DashboardUsageBar({ value }: { value: number }) {
  const clamped = Math.min(100, Math.max(0, value));
  const fill =
    clamped > 80 ? status.danger : clamped > 50 ? status.warn : brand.accent;

  return (
    <div className="bar mt-2">
      <span style={{ width: `${clamped}%`, background: fill }} />
    </div>
  );
}
