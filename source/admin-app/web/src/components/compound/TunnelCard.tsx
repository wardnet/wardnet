import { ArrowDown, ArrowUp, Loader2 } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Button } from "@wardnet/forge-web/button";
import { StatusBadge } from "./StatusBadge";
import { formatBytes, timeAgo } from "@/lib/utils";
import { countryFlag } from "@/lib/country";
import { useTestTunnel } from "@/hooks/useTunnels";
import type { Tunnel, TunnelStatus, ProviderInfo, TunnelTestResult } from "@wardnet/js";

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

/**
 * Hover text explaining a non-`up` tunnel status. `connecting` means the
 * iface is configured but the peer hasn't replied yet; `reconnecting`
 * means we previously had a handshake and it's gone stale (>3 min).
 */
function statusTooltip(tunnel: Tunnel): string | undefined {
  switch (tunnel.status) {
    case "connecting":
      return "Waiting for first handshake from peer.";
    case "reconnecting":
      return tunnel.last_handshake
        ? `Last handshake ${timeAgo(tunnel.last_handshake)} — peer not responding.`
        : "Lost handshake — peer not responding.";
    case "up":
    case "down":
      return undefined;
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

  const testTunnel = useTestTunnel();
  const [testResult, setTestResult] = useState<TunnelTestResult | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const [testedAt, setTestedAt] = useState<string | null>(null);

  const onTestClick = (e: React.MouseEvent) => {
    // Card is wrapped in <Link>; don't navigate when clicking Test.
    e.preventDefault();
    e.stopPropagation();
    testTunnel.mutate(tunnel.id, {
      onSuccess: (data) => {
        setTestResult(data.result);
        setTestError(null);
        setTestedAt(new Date().toISOString());
      },
      onError: (err) => {
        setTestResult(null);
        setTestError(err instanceof Error ? err.message : "Tunnel test failed");
        setTestedAt(new Date().toISOString());
      },
    });
  };

  return (
    <Card className="transition-colors hover:bg-accent/30">
      <Link
        to={`/tunnels/${tunnel.id}`}
        className="block focus:outline-none focus-visible:ring-2 focus-visible:ring-accent rounded-xl"
      >
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
          <StatusBadge tone={statusTone(tunnel.status)} title={statusTooltip(tunnel)}>
            {statusLabel(tunnel.status)}
          </StatusBadge>
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
        </CardContent>
      </Link>
      <CardContent className="pt-0">
        {testResult && (
          <div className="mb-3 rounded-md border bg-muted/40 px-3 py-2 text-xs">
            <div className="flex items-center gap-2">
              <span aria-hidden>{countryFlag(testResult.country_code)}</span>
              <span className="font-medium">{testResult.country_code.toUpperCase()}</span>
              <span className="text-muted-foreground">·</span>
              <span className="font-mono">{testResult.exit_ip}</span>
              <span className="text-muted-foreground">·</span>
              <span>{testResult.latency_ms} ms</span>
            </div>
            {testedAt && <p className="mt-0.5 text-muted-foreground">tested {timeAgo(testedAt)}</p>}
          </div>
        )}
        {testError && !testResult && (
          <div className="mb-3 rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {testError}
          </div>
        )}
        <div className="flex justify-end gap-2">
          <Button variant="outline" disabled={testTunnel.isPending} onClick={onTestClick}>
            {testTunnel.isPending ? (
              <>
                <Loader2 className="mr-1 size-3 animate-spin" aria-hidden />
                Testing
              </>
            ) : (
              "Test"
            )}
          </Button>
          <Button
            variant="destructive"
            onClick={(e) => {
              // Card is wrapped in <Link>; don't navigate when clicking delete.
              e.preventDefault();
              e.stopPropagation();
              onDelete(tunnel.id);
            }}
          >
            Delete
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
