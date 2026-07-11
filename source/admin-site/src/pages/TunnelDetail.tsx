import { useState } from "react";
import { useParams, useNavigate } from "react-router";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Button } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import { Tabs, TabsList, TabsTrigger } from "@wardnet/web";
import { Heading, Text } from "@wardnet/web";
import { DetailPageHeader } from "@/components/compound/DetailPageHeader";
import { StatusBadge } from "@/components/compound/StatusBadge";
import { TunnelDevicesTable } from "@/components/features/TunnelDevicesTable";
import { TunnelThroughputChart } from "@/components/features/TunnelThroughputChart";
import { TunnelLatencyChart } from "@/components/features/TunnelLatencyChart";
import {
  useTunnel,
  useDeleteTunnel,
  useSetTunnelDnsOverride,
  useRebuildTunnel,
  useSpeedTestResults,
} from "@wardnet/web";
import { useTunnelStats, RANGES, type StatsRange } from "@wardnet/web";
import { type ZoomRange } from "@/hooks/useChartZoom";
import { countryFlag } from "@wardnet/web";
import { retentionPct, timeAgo } from "@wardnet/web";
import type { TunnelStatus } from "@wardnet/js";

/** Last-5 speed test history: each row pairs the tunnel value with its
 *  direct (WAN) baseline so the comparison is visible at a glance. */
function SpeedTestHistory({ tunnelId }: { tunnelId: string }) {
  const { data } = useSpeedTestResults(tunnelId);
  const results = data?.results ?? [];
  if (results.length === 0) return null;

  return (
    <Card data-testid="tunnel-speed-test-history">
      <CardHeader>
        <CardTitle>Speed test history</CardTitle>
      </CardHeader>
      <CardContent>
        <table className="w-full text-sm">
          <thead>
            <tr className="text-left text-ink-3">
              <th className="pb-2 font-medium">Tested at</th>
              <th className="pb-2 font-medium">Download (tun / direct)</th>
              <th className="pb-2 font-medium">Latency (tun / direct)</th>
              <th className="pb-2 font-medium">Kept</th>
            </tr>
          </thead>
          <tbody>
            {results.map((r) => (
              <tr key={r.id} className="border-t border-[var(--line)]">
                <td className="py-2">{timeAgo(r.tested_at)}</td>
                <td className="py-2 mono">
                  {r.tunnel_throughput_mbps.toFixed(1)} /{" "}
                  {r.direct_throughput_mbps.toFixed(1)} Mbps
                </td>
                <td className="py-2 mono">
                  {Math.round(r.tunnel_latency_ms)} /{" "}
                  {Math.round(r.direct_latency_ms)} ms
                </td>
                <td className="py-2">
                  {retentionPct(
                    r.direct_throughput_mbps,
                    r.tunnel_throughput_mbps,
                  )}
                  %
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </CardContent>
    </Card>
  );
}

function statusTone(status: TunnelStatus): "success" | "neutral" | "danger" {
  switch (status) {
    case "up":
      return "success";
    case "reconnecting":
      return "danger";
    case "down":
    case "connecting":
      return "neutral";
  }
}

function statusLabel(status: TunnelStatus): string {
  switch (status) {
    case "up":
      return "Active";
    case "down":
      return "Down";
    case "connecting":
      return "Connecting";
    case "reconnecting":
      return "Reconnecting";
  }
}

export default function TunnelDetail() {
  const { id = "" } = useParams<{ id: string }>();
  const [range, setRange] = useState<StatsRange>("24h");
  const [zoom, setZoom] = useState<ZoomRange | null>(null);
  const navigate = useNavigate();
  const { data, isLoading, isError } = useTunnel(id);
  const deleteTunnel = useDeleteTunnel();
  const rebuildTunnel = useRebuildTunnel();
  const setDnsOverride = useSetTunnelDnsOverride();
  const {
    data: statsData,
    isLoading: statsLoading,
    isError: statsError,
  } = useTunnelStats(id, range);

  if (isLoading) {
    return (
      <Text as="p" size="sm" className="text-ink-3">
        Loading…
      </Text>
    );
  }

  if (isError || !data) {
    return (
      <div className="col gap-8">
        <Heading level={1} size="3xl" className="text-ink">
          Tunnel not found
        </Heading>
        <Text as="p" size="sm" className="mt-1 text-ink-3">
          The tunnel you're looking for may have been deleted.
        </Text>
      </div>
    );
  }

  const tunnel = data.tunnel;
  const flag = tunnel.country_code ? countryFlag(tunnel.country_code) : "";

  return (
    <div className="col gap-20">
      <DetailPageHeader
        parentLabel="Tunnels"
        parentTo="/tunnels"
        itemLabel={`${flag ? `${flag} ` : ""}${tunnel.label}`}
        status={
          <StatusBadge tone={statusTone(tunnel.status)}>
            {statusLabel(tunnel.status)}
          </StatusBadge>
        }
        meta={
          <span>
            Last handshake:{" "}
            {tunnel.last_handshake ? timeAgo(tunnel.last_handshake) : "never"}
          </span>
        }
      />

      <Card>
        <CardHeader>
          <CardTitle>Configuration</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-5">
          <div className="grid grid-cols-2 gap-x-6 md:grid-cols-4">
            <Field
              label="Provider"
              editing={false}
              value={tunnel.provider ?? "-"}
            />
            <Field
              label="Country"
              editing={false}
              value={`${flag ? `${flag} ` : ""}${tunnel.country_code?.toUpperCase() ?? "-"}`}
            />
            <Field
              label="Endpoint"
              editing={false}
              value={
                <span className="mono block truncate" title={tunnel.endpoint}>
                  {tunnel.endpoint}
                </span>
              }
            />
            <Field
              label="Interface"
              editing={false}
              value={<span className="mono">{tunnel.interface_name}</span>}
            />
          </div>
          {tunnel.server_selector && (
            <div className="grid grid-cols-2 gap-x-6 md:grid-cols-3">
              <Field
                label="Server selector"
                editing={false}
                value={`Best ${tunnel.server_selector.country.toUpperCase()}`}
              />
              {tunnel.resolved_server_name && (
                <Field
                  label="Resolved server"
                  editing={false}
                  value={tunnel.resolved_server_name}
                />
              )}
              {tunnel.endpoint_resolved_at && (
                <Field
                  label="Last resolved"
                  editing={false}
                  value={timeAgo(tunnel.endpoint_resolved_at)}
                />
              )}
            </div>
          )}
          <Field
            direction="row"
            label="Filter and route DNS through this tunnel"
            htmlFor="dns-override"
            help="When on, devices routed through this tunnel resolve DNS via Wardnet so ad-blocking still applies, and Wardnet forwards those queries via the tunnel interface using the tunnel's DNS server. When off, the system-wide upstream pool is used."
          >
            <Toggle
              id="dns-override"
              checked={tunnel.override_default_dns}
              disabled={setDnsOverride.isPending}
              onCheckedChange={(checked) =>
                setDnsOverride.mutate({ id: tunnel.id, value: checked })
              }
            />
          </Field>
        </CardContent>
      </Card>

      <div className="col gap-4">
        <div className="flex justify-end">
          <Tabs
            value={range}
            onValueChange={(v) => {
              setRange(v as StatsRange);
              setZoom(null);
            }}
          >
            <TabsList>
              {RANGES.map((r) => (
                <TabsTrigger key={r.value} value={r.value}>
                  {r.label}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
        </div>
        <TunnelThroughputChart
          data={statsData}
          isLoading={statsLoading}
          isError={statsError}
          range={range}
          zoom={zoom}
          onZoomChange={setZoom}
        />
        <TunnelLatencyChart
          data={statsData}
          isLoading={statsLoading}
          isError={statsError}
          range={range}
          zoom={zoom}
          onZoomChange={setZoom}
        />
      </div>

      <TunnelDevicesTable tunnelId={tunnel.id} />

      <SpeedTestHistory tunnelId={tunnel.id} />

      <div className="row justify-end gap-8">
        <Button
          variant="outline"
          disabled={rebuildTunnel.isPending}
          onClick={() => rebuildTunnel.mutate(tunnel.id)}
        >
          {rebuildTunnel.isPending ? "Rebuilding…" : "Rebuild tunnel"}
        </Button>
        <Button
          variant="destructive"
          data-testid="tunnel-detail-delete"
          onClick={() => {
            deleteTunnel.mutate(tunnel.id, {
              onSuccess: () => navigate("/tunnels"),
            });
          }}
        >
          Delete tunnel
        </Button>
      </div>
    </div>
  );
}
