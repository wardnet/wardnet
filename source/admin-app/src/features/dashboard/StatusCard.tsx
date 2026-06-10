import { formatUptime } from "@wardnet/web";

interface Props {
  reachable: boolean;
  uptimeSeconds: number | null;
}

export function StatusCard({ reachable, uptimeSeconds }: Props) {
  const statusColor = reachable ? "var(--color-accent)" : "var(--color-danger)";

  return (
    <div
      className="rounded-xl px-4 py-4 flex flex-col gap-4"
      style={{
        background: `radial-gradient(ellipse at 75% 40%, color-mix(in srgb, ${statusColor} 18%, transparent) 0%, transparent 60%), var(--color-side)`,
      }}
    >
      {/* Status badge */}
      <div
        className="self-start flex items-center gap-2 rounded-full px-3 py-1.5 text-[11px] font-semibold uppercase tracking-widest"
        style={{
          background: `color-mix(in srgb, ${statusColor} 14%, var(--color-side))`,
          color: statusColor,
        }}
      >
        <span
          className="size-[7px] rounded-full shrink-0"
          style={{ background: statusColor }}
        />
        {reachable ? "All Systems Healthy" : "Daemon Unreachable"}
      </div>

      {/* Health indicators */}
      <div className="flex gap-6">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-wider text-side-ink-2">
            Uptime
          </div>
          <div className="mt-0.5 text-sm font-medium text-side-ink">
            {uptimeSeconds != null ? formatUptime(uptimeSeconds) : "—"}
          </div>
        </div>
      </div>
    </div>
  );
}
