import { formatUptime, Text } from "@wardnet/web";

interface Props {
  reachable: boolean;
  uptimeSeconds: number | null;
}

export function StatusCard({ reachable, uptimeSeconds }: Props) {
  const statusColor = reachable ? "var(--color-accent)" : "var(--color-danger)";

  return (
    <div
      data-testid="dashboard-status-card"
      className="rounded-xl px-4 py-4 flex flex-col gap-4"
      style={{
        background: `radial-gradient(ellipse at 75% 40%, color-mix(in srgb, ${statusColor} 18%, transparent) 0%, transparent 60%), var(--color-side)`,
      }}
    >
      {/* Status badge */}
      <Text
        as="div"
        size="2xs"
        weight="semibold"
        className="self-start flex items-center gap-2 rounded-full px-3 py-1.5 uppercase tracking-widest"
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
      </Text>

      {/* Health indicators */}
      <div className="flex gap-6">
        <div>
          <Text
            as="div"
            size="2xs"
            weight="semibold"
            className="uppercase tracking-wider text-side-ink-2"
          >
            Uptime
          </Text>
          <Text
            as="div"
            size="sm"
            weight="medium"
            className="mt-0.5 text-side-ink"
          >
            {uptimeSeconds != null ? formatUptime(uptimeSeconds) : "—"}
          </Text>
        </div>
      </div>
    </div>
  );
}
