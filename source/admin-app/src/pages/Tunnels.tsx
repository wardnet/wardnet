import { memo, useState } from "react";
import { ChevronDownIcon, RotateCcwIcon, ZapIcon } from "lucide-react";
import { Card, Text, Heading } from "@wardnet/web";
import { Sparkline } from "@wardnet/web";
import {
  useTunnels,
  useDevices,
  useDefaultPolicy,
  useRebuildTunnel,
  useTunnelStats,
  useCombinedTunnelStats,
  useSpeedTestResults,
  useStartSpeedTest,
  countryFlag,
  formatBytes,
  retentionPct,
  timeAgo,
  TunnelStatusPill,
} from "@wardnet/web";
import { useOnlineStatusContext } from "@/context/OnlineStatusContext";
import type { Device, Tunnel, TunnelSpeedTestResult } from "@wardnet/js";

const BUCKET_SECS = 60;

/** Compact mobile speed-test panel: a run button, the latest direct-vs-tunnel
 *  result inline, and tap-to-expand for the last-5 history (no detail route). */
const SpeedTestPanel = memo(function SpeedTestPanel({
  tunnelId,
}: {
  tunnelId: string;
}) {
  const startSpeedTest = useStartSpeedTest();
  const [requested, setRequested] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const running =
    startSpeedTest.isRunning && startSpeedTest.activeTunnelId === tunnelId;
  // Only fetch history once the user runs a test on this card (or while one is
  // in flight) — avoids one request per card across the tunnels list on load.
  const { data } = useSpeedTestResults(tunnelId, requested || running);

  const results = data?.results ?? [];
  const latest: TunnelSpeedTestResult | undefined = results[0];

  return (
    <div className="flex flex-col gap-2 border-t border-line pt-3">
      <button
        data-testid="tunnel-speed-test-button"
        onClick={() => {
          setRequested(true);
          startSpeedTest.start(tunnelId);
        }}
        disabled={running}
        className="flex h-9 items-center justify-center gap-1.5 rounded-lg bg-sunken text-ink-2 transition-colors duration-snap active:bg-line disabled:opacity-50"
      >
        <ZapIcon size={14} className={running ? "animate-pulse" : undefined} />
        <Text as="span" size="sm" weight="medium">
          {running ? `Testing ${startSpeedTest.percentage}%` : "Speed test"}
        </Text>
      </button>

      {latest && (
        <div
          data-testid="tunnel-speed-test-result"
          className="flex flex-col rounded-lg bg-sunken"
        >
          <button
            onClick={() => setExpanded((v) => !v)}
            className="flex flex-col gap-1 px-3 py-2 text-left"
          >
            <div className="flex items-center justify-between">
              <Text as="span" size="xs" className="text-ink-3">
                ↓ tun / direct
              </Text>
              <div className="flex items-center gap-2">
                <Text
                  as="span"
                  size="sm"
                  weight="medium"
                  className="font-mono text-ink"
                >
                  {latest.tunnel_throughput_mbps.toFixed(1)} /{" "}
                  {latest.direct_throughput_mbps.toFixed(1)} Mbps
                </Text>
                <Text
                  as="span"
                  size="xs"
                  weight="medium"
                  className="text-ink-2"
                >
                  {retentionPct(
                    latest.direct_throughput_mbps,
                    latest.tunnel_throughput_mbps,
                  )}
                  % kept
                </Text>
              </div>
            </div>
            <div className="flex items-center justify-between">
              <Text as="span" size="xs" className="text-ink-3">
                ◷ tun / direct
              </Text>
              <div className="flex items-center gap-2">
                <Text as="span" size="sm" className="font-mono text-ink">
                  {Math.round(latest.tunnel_latency_ms)} /{" "}
                  {Math.round(latest.direct_latency_ms)} ms
                </Text>
                <ChevronDownIcon
                  size={14}
                  className={`text-ink-3 transition-transform duration-snap ${
                    expanded ? "rotate-180" : ""
                  }`}
                />
              </div>
            </div>
            <Text as="span" size="2xs" className="text-ink-3">
              tested {timeAgo(latest.tested_at)}
            </Text>
          </button>

          {expanded && results.length > 1 && (
            <div
              data-testid="tunnel-speed-test-history"
              className="flex flex-col gap-1 border-t border-line px-3 py-2"
            >
              {results.slice(1).map((r) => (
                <div key={r.id} className="flex items-center justify-between">
                  <Text as="span" size="xs" className="font-mono text-ink-2">
                    {r.tunnel_throughput_mbps.toFixed(1)} /{" "}
                    {r.direct_throughput_mbps.toFixed(1)} Mbps
                  </Text>
                  <Text as="span" size="xs" className="text-ink-3">
                    {retentionPct(
                      r.direct_throughput_mbps,
                      r.tunnel_throughput_mbps,
                    )}
                    % · {timeAgo(r.tested_at)}
                  </Text>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
});

function deviceCount(
  tunnelId: string,
  devices: Device[],
  defaultPolicy: string | undefined,
): number {
  const explicit = devices.filter(
    (d) =>
      d.current_rule?.type === "tunnel" &&
      d.current_rule.tunnel_id === tunnelId,
  ).length;
  const onDefault =
    defaultPolicy === tunnelId
      ? devices.filter(
          (d) => d.current_rule === null || d.current_rule.type === "default",
        ).length
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
      <div data-testid="tunnel-card" className="flex flex-col gap-3 p-4">
        {/* Header row */}
        <div className="flex items-start gap-2">
          <Text as="span" size="2xl" className="leading-none" aria-hidden>
            {countryFlag(tunnel.country_code)}
          </Text>
          <div className="min-w-0 flex-1">
            <Text as="p" weight="semibold" className="truncate text-ink">
              {tunnel.resolved_server_name ?? tunnel.label}
            </Text>
            <Text as="p" size="xs" className="truncate text-ink-3">
              {subtitle}
            </Text>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <TunnelStatusPill status={tunnel.status} />
            <button
              data-testid="tunnel-rebuild"
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
            <Text
              as="p"
              size="2xs"
              weight="semibold"
              className="uppercase tracking-wider text-ink-3"
            >
              Endpoint
            </Text>
            <Text
              as="p"
              size="sm"
              className="mt-0.5 break-all font-mono text-ink"
            >
              {tunnel.endpoint}
            </Text>
          </div>
          <div>
            <Text
              as="p"
              size="2xs"
              weight="semibold"
              className="uppercase tracking-wider text-ink-3"
            >
              Last Handshake
            </Text>
            <Text as="p" size="sm" className="mt-0.5 text-ink">
              {tunnel.last_handshake ? timeAgo(tunnel.last_handshake) : "-"}
            </Text>
          </div>
          <div>
            <Text
              as="p"
              size="2xs"
              weight="semibold"
              className="uppercase tracking-wider text-ink-3"
            >
              Data ↓ / ↑
            </Text>
            <Text as="p" size="sm" className="mt-0.5 text-ink">
              {formatBytes(tunnel.bytes_rx)} · {formatBytes(tunnel.bytes_tx)}
            </Text>
          </div>
          <div>
            <Text
              as="p"
              size="2xs"
              weight="semibold"
              className="uppercase tracking-wider text-ink-3"
            >
              Devices
            </Text>
            <Text as="p" size="sm" className="mt-0.5 text-ink">
              {devCount} routed
            </Text>
          </div>
        </div>

        {/* Live throughput — up tunnels only */}
        {tunnel.status === "up" && (
          <div className="flex items-center gap-3 border-t border-line pt-3">
            <Text as="span" size="xs" className="text-ink-3">
              ↓ {formatBytes(rxRate)}/s
            </Text>
            <Text as="span" size="xs" className="text-ink-3">
              ↑ {formatBytes(txRate)}/s
            </Text>
          </div>
        )}

        <SpeedTestPanel tunnelId={tunnel.id} />
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
          <Heading level={1} size="3xl" weight="bold" className="text-ink">
            Tunnels
          </Heading>
          <Text as="p" size="base" className="text-ink-3">
            VPN tunnel status and throughput.
          </Text>
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
        <Heading level={1} size="3xl" weight="bold" className="text-ink">
          Tunnels
        </Heading>
        <Text as="p" size="base" className="text-ink-3">
          VPN tunnel status and throughput.
        </Text>
      </div>

      <div
        className={
          showingLastKnownState
            ? "pointer-events-none opacity-40 transition-opacity"
            : "transition-opacity"
        }
      >
        <div className="flex flex-col gap-4">
          {/* Summary header */}
          <Card className="card--flush">
            <div
              data-testid="tunnels-summary"
              className="flex items-center gap-3 p-4"
            >
              <div className="min-w-0 shrink-0">
                <Text
                  as="div"
                  size="2xs"
                  weight="semibold"
                  className="uppercase tracking-wider text-ink-3"
                >
                  Tunnels Up
                </Text>
                <div className="mt-0.5 flex items-baseline gap-1.5">
                  <Text
                    as="span"
                    size="2xl"
                    weight="bold"
                    className="text-ink tabular-nums"
                  >
                    {upCount}
                  </Text>
                  <Text as="span" size="sm" className="text-ink-3">
                    / {tunnels.length}
                  </Text>
                </div>
                <Text as="div" size="xs" className="mt-0.5 text-ink-3">
                  ↓ {formatBytes(combinedRxRate)}/s · ↑{" "}
                  {formatBytes(combinedTxRate)}/s
                </Text>
              </div>

              <div className="h-12 flex-1 opacity-70">
                {sparkValues.length > 0 && (
                  <Sparkline
                    values={sparkValues}
                    color="var(--color-info)"
                    area={false}
                  />
                )}
              </div>
            </div>
          </Card>

          {/* Per-tunnel cards */}
          {tunnels.length === 0 ? (
            <Text as="p" size="sm" className="py-16 text-center text-ink-3">
              No tunnels configured.
            </Text>
          ) : (
            <div className="flex flex-col gap-3">
              {tunnels.map((tunnel) => (
                <TunnelCard
                  key={tunnel.id}
                  tunnel={tunnel}
                  devices={devices}
                  defaultPolicy={defaultPolicy}
                  onRebuild={rebuild.mutate}
                  rebuildingId={
                    rebuild.isPending ? rebuild.variables : undefined
                  }
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
