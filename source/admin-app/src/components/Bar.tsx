/** variant="usage" — high percent is bad (CPU, memory, disk): danger ≥90, warn ≥70, accent below.
 *  variant="rate"  — high percent is good (cache hit rate):   accent ≥80, warn ≥50, danger below. */
export function Bar({
  percent,
  variant = "usage",
}: {
  percent: number;
  variant?: "usage" | "rate";
}) {
  const clamped = Math.min(100, Math.max(0, percent));
  const color =
    variant === "rate"
      ? clamped >= 80 ? "bg-accent" : clamped >= 50 ? "bg-warn" : "bg-danger"
      : clamped >= 90 ? "bg-danger" : clamped >= 70 ? "bg-warn" : "bg-accent";
  return (
    <div className="mt-1.5 h-1 overflow-hidden rounded-full bg-line">
      <div
        className={`h-full rounded-full transition-all ${color}`}
        style={{ width: `${clamped}%` }}
      />
    </div>
  );
}
