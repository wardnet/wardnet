/** DHCP-adjacent display helpers shared across DHCP cards and pages. */

/**
 * Pool utilisation as an integer percentage, clamped to 0–100.
 *
 * Shrinking the pool below the active-lease count is a documented, handled
 * scenario, so `used` can legitimately exceed `total`. Clamping here keeps a
 * displayed percentage in step with a usage bar — which is itself capped at
 * 100% — instead of rendering e.g. "120% pool used" next to a full bar. A
 * non-positive `total` (no pool configured) reads as 0% rather than dividing
 * by zero.
 */
export function poolUsagePercent(used: number, total: number): number {
  if (total <= 0) return 0;
  return Math.min(100, Math.max(0, Math.round((used / total) * 100)));
}
