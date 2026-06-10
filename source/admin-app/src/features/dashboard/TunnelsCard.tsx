import { NetworkIcon } from "lucide-react";
import { Link } from "react-router";
import { Card } from "@wardnet/web";
import { formatBytes, countryFlag } from "@wardnet/web";
import type { Tunnel } from "@wardnet/js";

interface Props {
  tunnelCount: number;
  tunnelActiveCount: number;
  tunnels: Tunnel[] | undefined;
}

export function TunnelsCard({ tunnelCount, tunnelActiveCount, tunnels }: Props) {
  const activeTunnels = tunnels?.filter((t) => t.status === "up") ?? [];
  const primary = activeTunnels[0];

  return (
    <Link to="/tunnels" className="block">
      <Card className="card--flush">
      <div className="flex items-center gap-3 px-2 py-3">
        <div
          className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
          style={{ background: "var(--color-info-soft)", color: "var(--color-info)" }}
        >
          <NetworkIcon size={22} strokeWidth={1.8} />
        </div>

        <div className="flex-1 min-w-0">
          <div className="text-[10px] font-semibold uppercase tracking-wider text-ink-3">
            Tunnels Up
          </div>
          <div className="mt-0.5 flex items-baseline gap-1.5">
            <span className="text-2xl font-bold text-ink">{tunnelActiveCount}</span>
            <span className="text-sm text-ink-3">/ {tunnelCount}</span>
          </div>
          {primary && (
            <div className="mt-0.5 flex items-center gap-1 text-xs text-ink-3 truncate">
              <span aria-hidden>{countryFlag(primary.country_code)}</span>
              <span className="truncate">
                {primary.resolved_server_name ?? primary.label}
              </span>
              <span>·</span>
              <span className="shrink-0">↓ {formatBytes(primary.bytes_rx)}</span>
            </div>
          )}
          {activeTunnels.length > 1 && (
            <div className="mt-0.5 text-xs text-ink-3">
              +{activeTunnels.length - 1} more
            </div>
          )}
        </div>
      </div>
    </Card>
    </Link>
  );
}
