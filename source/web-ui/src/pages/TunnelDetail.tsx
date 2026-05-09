import { useState } from "react";
import { useParams } from "react-router";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Button } from "@wardnet/forge/button";
import { DetailPageHeader } from "@/components/compound/DetailPageHeader";
import { StatusBadge } from "@/components/compound/StatusBadge";
import { TunnelDevicesTable } from "@/components/features/TunnelDevicesTable";
import { TunnelThroughputChart } from "@/components/features/TunnelThroughputChart";
import { useTunnel, useDeleteTunnel } from "@/hooks/useTunnels";
import { useNavigate } from "react-router";
import { countryFlag } from "@/lib/country";
import { timeAgo } from "@/lib/utils";
import type { TunnelMetricsRange, TunnelStatus } from "@wardnet/js";

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

interface MetadataRowProps {
  label: string;
  children: React.ReactNode;
}

function MetadataRow({ label, children }: MetadataRowProps) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs uppercase tracking-wide text-muted-foreground">{label}</span>
      <span className="text-sm">{children}</span>
    </div>
  );
}

export default function TunnelDetail() {
  const { id = "" } = useParams<{ id: string }>();
  const [range, setRange] = useState<TunnelMetricsRange>("24h");
  const navigate = useNavigate();
  const { data, isLoading, isError } = useTunnel(id);
  const deleteTunnel = useDeleteTunnel();

  if (isLoading) {
    return (
      <div className="p-6">
        <p className="text-sm text-muted-foreground">Loading…</p>
      </div>
    );
  }

  if (isError || !data) {
    return (
      <div className="flex flex-col gap-2 p-6">
        <h1 className="text-xl font-semibold">Tunnel not found</h1>
        <p className="text-sm text-muted-foreground">
          The tunnel you're looking for may have been deleted.
        </p>
      </div>
    );
  }

  const tunnel = data.tunnel;
  const flag = tunnel.country_code ? countryFlag(tunnel.country_code) : "";

  return (
    <div className="flex flex-col gap-6 p-4 md:p-6">
      <DetailPageHeader
        parentLabel="Tunnels"
        parentTo="/tunnels"
        itemLabel={`${flag ? `${flag} ` : ""}${tunnel.label}`}
        status={
          <StatusBadge tone={statusTone(tunnel.status)}>{statusLabel(tunnel.status)}</StatusBadge>
        }
        meta={
          <span>
            Last handshake: {tunnel.last_handshake ? timeAgo(tunnel.last_handshake) : "never"}
          </span>
        }
      />

      <Card>
        <CardHeader>
          <CardTitle>Configuration</CardTitle>
        </CardHeader>
        <CardContent className="grid grid-cols-2 gap-x-6 gap-y-4 md:grid-cols-4">
          <MetadataRow label="Provider">{tunnel.provider ?? "—"}</MetadataRow>
          <MetadataRow label="Country">
            {flag ? `${flag} ` : ""}
            {tunnel.country_code?.toUpperCase() ?? "—"}
          </MetadataRow>
          <MetadataRow label="Endpoint">
            <span className="font-mono text-xs">{tunnel.endpoint}</span>
          </MetadataRow>
          <MetadataRow label="Interface">
            <span className="font-mono text-xs">{tunnel.interface_name}</span>
          </MetadataRow>
        </CardContent>
      </Card>

      <TunnelThroughputChart tunnelId={tunnel.id} range={range} onRangeChange={setRange} />

      <TunnelDevicesTable tunnelId={tunnel.id} />

      <div className="flex justify-end">
        <Button
          variant="destructive"
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
