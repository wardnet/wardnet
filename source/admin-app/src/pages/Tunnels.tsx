import { memo } from "react";
import { RotateCcwIcon } from "lucide-react";
import { Card } from "@wardnet/forge-web/card";
import { Pill } from "@wardnet/forge-web/pill";
import { Sparkline } from "@wardnet/forge-web/sparkline";
import {
  useTunnels,
  useDevices,
  useDefaultPolicy,
  useRebuildTunnel,
  useTunnelStats,
  useCombinedTunnelStats,
  countryFlag,
  formatBytes,
  timeAgo,
} from "@wardnet/wardnet-web";
import { useOnlineStatusContext } from "@/context/OnlineStatusContext";
import type { Device, Tunnel, TunnelStatus } from "@wardnet/js";

const BUCKET_SECS = 60;

function pillVariant(status: TunnelStatus) {
  switch (status) {
    case "up":
      return "ok" as const;
    case "connecting":
      return "info" as const;
    case "reconnecting":
      return "warn" as const;
    case "down":
      return "down" as const;
  }
}

function pillLabel(status: TunnelStatus) {
  switch (status) {
    case "up":
      return "Active";
    case "connecting":
      return "Connecting";
    case "reconnecting":
      return "Reconnecting";
    case "down":
      return "Down";
  }
}

function deviceCount(
  tunnelId: string,
  devices: Device[],
  defaultPolicy: string | undefined,
): number {
  const explicit = devices.filter(
    (d) => d.current_rule?.type === "tunnel" && d.current_rule.tunnel_id === tunnelId,
  ).length;
  const onDefault =
    defaultPolicy === tunnelId
      ? devices.filter((d) => d.current_rule === null || d.current_rule.type === "default").length
      : 0;
  return explicit + onDefault;
}

const TunnelCard = memo(function TunnelCard({
  tunnel,
  devices,
  defaultPolicy,
  onRebuild,
  rebuildingId,
}: {
  tunnel: Tunnel;
  devices: Device[];
  defaultPolicy: string | undefined;
  onRebuild: (id: string) => void;
  rebuildingId: string | undefined;
}) {
  const { data: stats } = useTunnelStats(tunnel.id, "1h");

  const subtitle = [tunnel.country_code, tunnel.provider, tunnel.interface_name]
    .filter(Boolean)
    .join(" · ");

  const devCount = deviceCount(tunnel.id, devices, defaultPolicy);

  const lastPoint = stats?.points[stats.points.length - 1];
  const rxRate = lastPoint ? lastPoint.bytesRx / BUCKET_SECS : 0;
  const txRate = lastPoint ? lastPoint.bytesTx / BUCKET_SECS : 0;

  const isRebuilding = rebuildingId === tunnel.id;

  return (
    <Card className="card--flush">
      <div className="flex flex-col gap-3 p-4">
        {/* Header row */}
        <div className="flex items-start gap-2">
          <span className="text-2xl leading-none" aria-hidden>
            {countryFlag(tunnel.country_code)}
          </span>
          <div className="min-w-0 flex-1">
            <p className="truncate font-semibold text-ink">
              {tunnel.resolved_server_name ?? tunnel.label}
            </p>
            <p className="truncate text-xs text-ink-3">{subtitle}</p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Pill variant={pillVariant(tunnel.status)}>
              <span className="mr-1" aria-hidden>●</span>
              {pillLabel(tunnel.status)}
            </Pill>
            <button
              onClick={() => onRebuild(tunnel.id)}
              disabled={isRebuilding}
              className="flex h-7 w-7 items-center justify-center rounded-lg bg-sunken text-ink-3 transition-colors duration-snap active:bg-line disabled:opacity-40"
              aria-label="Rebuild tunnel"
            >
              <RotateCcwIcon
                size={14}
                className={isRebuilding ? "animate-spin" : undefined}
              />
            </button>
          </div>
        </div>

        {/* 2×2 data grid */}
        <div className="grid grid-cols-2 gap-x-4 gap-y-3 border-t border-line pt-3">
          <div>
            <p className="text-[10px] font-semibold uppercase tracking-wider text-ink-3">
              Endpoint
            </p>
            <p className="mt-0.5 break-all font-mono text-[13px] text-ink">{tunnel.endpoint}</p>
          </div>
          <div>
            <p className="text-[10px] font-semibold uppercase tracking-wider text-ink-3">
              Last Handshake
            </p>
            <p className="mt-0.5 text-[13px] text-ink">
              {tunnel.last_handshake ? timeAgo(tunnel.last_handshake) : "—"}
            </p>
          </div>
          <div>
            <p className="text-[10px] font-semibold uppercase tracking-wider text-ink-3">
              Data ↓ / ↑
            </p>
            <p className="mt-0.5 text-[13px] text-ink">
              {formatBytes(tunnel.bytes_rx)} · {formatBytes(tunnel.bytes_tx)}
            </p>
          </div>
          <div>
            <p className="text-[10px] font-semibold uppercase tracking-wider text-ink-3">
              Devices
            </p>
            <p className="mt-0.5 text-[13px] text-ink">{devCount} routed</p>
          </div>
        </div>

        {/* Live throughput — up tunnels only */}
        {tunnel.status === "up" && (
          <div className="flex items-center gap-3 border-t border-line pt-3">
            <span className="text-xs text-ink-3">↓ {formatBytes(rxRate)}/s</span>
            <span className="text-xs text-ink-3">↑ {formatBytes(txRate)}/s</span>
          </div>
        )}
      </div>
    </Card>
  );
});

export default function Tunnels() {
  const { data: tunnelsData, isLoading } = useTunnels();
  const { data: devicesData } = useDevices();
  const { data: policyData } = useDefaultPolicy();
  const { showingLastKnownState } = useOnlineStatusContext();
  const rebuild = useRebuildTunnel();
  const { data: combinedStats } = useCombinedTunnelStats();

  const tunnels = tunnelsData?.tunnels ?? [];
  const devices = devicesData?.devices ?? [];
  const defaultPolicy = policyData?.policy;
  const upCount = tunnels.filter((t) => t.status === "up").length;

  const sparkValues = combinedStats?.sparkValues ?? [];
  const combinedRxRate = combinedStats?.rxRate ?? 0;
  const combinedTxRate = combinedStats?.txRate ?? 0;

  if (isLoading) {
    return (
      <div className="flex flex-col gap-4 p-4">
        <div>
          <h1 className="text-[28px] font-bold text-ink">Tunnels</h1>
          <p className="text-[14px] text-ink-3">VPN tunnel status and throughput.</p>
        </div>
        <div className="h-24 animate-pulse rounded-xl bg-sunken" />
        {Array.from({ length: 3 }).map((_, i) => (
          <div key={i} className="h-48 animate-pulse rounded-xl bg-sunken" />
        ))}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4 p-4">
      <div>
        <h1 className="text-[28px] font-bold text-ink">Tunnels</h1>
        <p className="text-[14px] text-ink-3">VPN tunnel status and throughput.</p>
      </div>

      <div className={showingLastKnownState ? "pointer-events-none opacity-40 transition-opacity" : "transition-opacity"}>
      <div className="flex flex-col gap-4">

      {/* Summary header */}
      <Card className="card--flush">
        <div className="flex items-center gap-3 p-4">
          <div className="min-w-0 shrink-0">
            <div className="text-[10px] font-semibold uppercase tracking-wider text-ink-3">
              Tunnels Up
            </div>
            <div className="mt-0.5 flex items-baseline gap-1.5">
              <span className="text-2xl font-bold text-ink tabular-nums">{upCount}</span>
              <span className="text-sm text-ink-3">/ {tunnels.length}</span>
            </div>
            <div className="mt-0.5 text-xs text-ink-3">
              ↓ {formatBytes(combinedRxRate)}/s · ↑ {formatBytes(combinedTxRate)}/s
            </div>
          </div>

          <div className="h-12 flex-1 opacity-70">
            {sparkValues.length > 0 && (
              <Sparkline values={sparkValues} color="var(--color-info)" area={false} />
            )}
          </div>
        </div>
      </Card>

      {/* Per-tunnel cards */}
      {tunnels.length === 0 ? (
        <p className="py-16 text-center text-sm text-ink-3">No tunnels configured.</p>
      ) : (
        <div className="flex flex-col gap-3">
          {tunnels.map((tunnel) => (
            <TunnelCard
              key={tunnel.id}
              tunnel={tunnel}
              devices={devices}
              defaultPolicy={defaultPolicy}
              onRebuild={rebuild.mutate}
              rebuildingId={rebuild.isPending ? rebuild.variables : undefined}
            />
          ))}
        </div>
      )}

      </div>
      </div>
    </div>
  );
}
