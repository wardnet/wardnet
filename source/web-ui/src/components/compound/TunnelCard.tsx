import { ArrowDown, ArrowUp } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Badge } from "@/components/core/ui/badge";
import { Button } from "@/components/core/ui/button";
import { formatBytes, timeAgo } from "@/lib/utils";
import { countryFlag } from "@/lib/country";
import type { Tunnel, TunnelStatus, ProviderInfo } from "@wardnet/js";

function statusColor(status: TunnelStatus) {
  const map = {
    up: "default",
    down: "secondary",
    connecting: "outline",
  } as const;
  return map[status];
}

function statusLabel(status: TunnelStatus): string {
  switch (status) {
    case "up":
      return "Active";
    case "down":
      return "Down";
    case "connecting":
      return "Connecting";
  }
}

/** Inline provider logo or letter fallback. */
function ProviderLogo({ provider }: { provider: ProviderInfo | undefined }) {
  if (!provider) return null;
  if (provider.icon_url) {
    return <img src={provider.icon_url} alt="" className="size-4 rounded-sm object-contain" />;
  }
  return (
    <span className="flex size-4 items-center justify-center rounded-sm bg-muted text-[10px] font-bold uppercase text-muted-foreground">
      {provider.name[0]}
    </span>
  );
}

interface TunnelCardProps {
  tunnel: Tunnel;
  providers: ProviderInfo[];
  onDelete: (id: string) => void;
}

/** Card displaying a single WireGuard tunnel with status and stats. */
export function TunnelCard({ tunnel, providers, onDelete }: TunnelCardProps) {
  const provider = providers.find((p) => p.id === tunnel.provider);
  const flag = tunnel.country_code ? countryFlag(tunnel.country_code) : "";

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <div className="flex items-center gap-3">
          {provider && <ProviderLogo provider={provider} />}
          <div className="flex flex-col gap-1">
            <CardTitle className="text-base">
              {flag && <span className="mr-1.5">{flag}</span>}
              {tunnel.label}
            </CardTitle>
            <p className="text-xs text-muted-foreground">
              {tunnel.country_code && tunnel.country_code.toUpperCase()}
              {provider && ` \u00b7 ${provider.name}`}
              {!provider && tunnel.provider && ` \u00b7 ${tunnel.provider}`}
            </p>
          </div>
        </div>
        <Badge variant={statusColor(tunnel.status)}>{statusLabel(tunnel.status)}</Badge>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-2 gap-y-2 text-sm">
          <div>
            <span className="text-muted-foreground">Interface</span>
            <p className="font-mono text-xs">{tunnel.interface_name}</p>
          </div>
          <div>
            <span className="text-muted-foreground">Endpoint</span>
            <p className="font-mono text-xs">{tunnel.endpoint}</p>
          </div>
          <div>
            <span className="text-muted-foreground">Traffic</span>
            <p className="flex items-center gap-2 text-xs">
              <span className="inline-flex items-center gap-0.5">
                <ArrowUp className="size-3" aria-label="up" />
                {formatBytes(tunnel.bytes_tx)}
              </span>
              <span className="inline-flex items-center gap-0.5">
                <ArrowDown className="size-3" aria-label="down" />
                {formatBytes(tunnel.bytes_rx)}
              </span>
            </p>
          </div>
          <div>
            <span className="text-muted-foreground">Last handshake</span>
            <p className="text-xs">
              {tunnel.last_handshake ? timeAgo(tunnel.last_handshake) : "\u2014"}
            </p>
          </div>
        </div>
        <div className="mt-4 flex justify-end">
          <Button variant="destructive" size="sm" onClick={() => onDelete(tunnel.id)}>
            Delete
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
