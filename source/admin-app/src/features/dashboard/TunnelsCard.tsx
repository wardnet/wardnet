import { NetworkIcon } from "lucide-react";
import { Card, CardHeader, CardContent } from "@wardnet/forge-web/card";
import { StatTile } from "@wardnet/forge-web/stat-tile";
import { Pill } from "@wardnet/forge-web/pill";
import { formatBytes, countryFlag } from "@wardnet/wardnet-web";
import type { Tunnel } from "@wardnet/js";

interface Props {
  tunnelCount: number;
  tunnelActiveCount: number;
  tunnels: Tunnel[] | undefined;
}

export function TunnelsCard({ tunnelCount, tunnelActiveCount, tunnels }: Props) {
  const activeTunnels = tunnels?.filter((t) => t.status === "up") ?? [];

  return (
    <Card>
      <CardHeader className="flex items-center gap-2 px-4 pt-4 pb-0 text-sm font-medium text-ink-2">
        <NetworkIcon size={14} strokeWidth={1.8} />
        Tunnels
      </CardHeader>
      <CardContent className="px-4 pb-4 flex flex-col gap-3">
        <StatTile
          label="Up"
          value={`${tunnelActiveCount} / ${tunnelCount}`}
          pill={
            tunnelActiveCount > 0 ? (
              <Pill variant="ok">Active</Pill>
            ) : (
              <Pill variant="down">Down</Pill>
            )
          }
        />
        {activeTunnels.length > 0 && (
          <ul className="flex flex-col gap-1.5">
            {activeTunnels.map((tunnel) => (
              <li key={tunnel.id} className="flex items-center justify-between text-sm">
                <span className="flex items-center gap-1.5 text-ink">
                  <span aria-hidden>{countryFlag(tunnel.country_code)}</span>
                  <span className="truncate max-w-[160px]">
                    {tunnel.resolved_server_name ?? tunnel.label}
                  </span>
                </span>
                <span className="font-mono text-xs text-ink-3 shrink-0">
                  ↓ {formatBytes(tunnel.bytes_rx)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
