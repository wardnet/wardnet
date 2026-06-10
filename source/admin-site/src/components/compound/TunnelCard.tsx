import {
  ArrowDown,
  ArrowUp,
  Loader2,
  RotateCcw,
  SlidersHorizontal,
} from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";
import { Button } from "@wardnet/web";
import { StatusBadge } from "./StatusBadge";
import { countryFlag, formatBytes, timeAgo } from "@wardnet/web";
import { useTestTunnel, useRebuildTunnel } from "@wardnet/web";
import type {
  Tunnel,
  TunnelStatus,
  ProviderInfo,
  TunnelTestResult,
} from "@wardnet/js";

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

interface TunnelCardProps {
  tunnel: Tunnel;
  providers: ProviderInfo[];
  onDelete: (id: string) => void;
}

/** Card displaying a single WireGuard tunnel with status and stats.
 *  Consumes the Forge `.tcard` family (`.tcard__head` / `.tcard__flag` /
 *  `.tcard__title` / `.tcard__sub` / `.tcard__grid` / `.tcard__throughput`)
 *  per the studio mock in `forge/docs/screens.jsx`. */
export function TunnelCard({ tunnel, providers, onDelete }: TunnelCardProps) {
  const provider = providers.find((p) => p.id === tunnel.provider);
  const flag = tunnel.country_code ? countryFlag(tunnel.country_code) : "";

  const testTunnel = useTestTunnel();
  const rebuildTunnel = useRebuildTunnel();
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

  const subParts: string[] = [];
  if (tunnel.country_code) subParts.push(tunnel.country_code.toUpperCase());
  if (provider) subParts.push(provider.name);
  else if (tunnel.provider) subParts.push(tunnel.provider);

  return (
    <div className="tcard">
      <Link
        to={`/tunnels/${tunnel.id}`}
        className="col gap-16 rounded-[var(--radius-lg)] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent"
      >
        <div className="tcard__head">
          {/* Row 1 — chips on the left, status pill on the right. */}
          <div className="tcard__chips">
            <div className="tcard__provider" aria-hidden>
              {provider?.icon_url ? (
                <img src={provider.icon_url} alt="" />
              ) : (
                <SlidersHorizontal aria-label="Custom configuration" />
              )}
            </div>
            {flag && (
              <div className="tcard__flag" aria-hidden>
                {flag}
              </div>
            )}
            {tunnel.server_selector && (
              <span
                className="rounded-sm bg-sunken px-1.5 py-0.5 text-[10px] font-medium text-ink-3"
                aria-label={`Auto-selected best server in ${tunnel.server_selector.country.toUpperCase()}`}
              >
                Best {tunnel.server_selector.country.toUpperCase()}
              </span>
            )}
            <div className="spacer" />
            <StatusBadge
              tone={statusTone(tunnel.status)}
              title={statusTooltip(tunnel)}
            >
              <span className="dot" />
              {statusLabel(tunnel.status)}
            </StatusBadge>
          </div>
          {/* Row 2 — full-width title + sub. */}
          <div className="tcard__text">
            <div className="tcard__title">{tunnel.label}</div>
            <div className="tcard__sub">
              {subParts.length > 0 && <>{subParts.join(" · ")} · </>}
              <span className="mono">{tunnel.interface_name}</span>
            </div>
          </div>
        </div>

        <dl className="tcard__grid">
          <div>
            <dt>Endpoint</dt>
            <dd title={tunnel.endpoint}>{tunnel.endpoint}</dd>
          </div>
          <div>
            <dt>Last handshake</dt>
            <dd>
              {tunnel.last_handshake ? timeAgo(tunnel.last_handshake) : "—"}
            </dd>
          </div>
        </dl>

        <div>
          <div className="tcard__label">Data transfer</div>
          <div className="tcard__throughput" aria-label="Tunnel throughput">
            <span>
              <ArrowUp aria-label="up" />
              <span className="mono">{formatBytes(tunnel.bytes_tx)}</span>
            </span>
            <span>
              <ArrowDown aria-label="down" />
              <span className="mono">{formatBytes(tunnel.bytes_rx)}</span>
            </span>
          </div>
        </div>
      </Link>

      {testResult && (
        <div className="row gap-8 rounded-[var(--radius-md)] border border-[var(--line)] bg-[var(--bg-sunken)] px-3 py-2 text-xs">
          <span aria-hidden>{countryFlag(testResult.country_code)}</span>
          <span className="font-medium">
            {testResult.country_code.toUpperCase()}
          </span>
          <span className="text-ink-3">·</span>
          <span className="mono">{testResult.exit_ip}</span>
          <span className="text-ink-3">·</span>
          <span className="mono">{testResult.latency_ms} ms</span>
          {testedAt && (
            <span className="ml-auto text-ink-3">
              tested {timeAgo(testedAt)}
            </span>
          )}
        </div>
      )}
      {testError && !testResult && (
        <div
          role="alert"
          className="rounded-[var(--radius-md)] border border-[var(--danger-soft)] bg-[var(--danger-soft)] px-3 py-2 text-xs text-[var(--danger-soft-ink)]"
        >
          {testError}
        </div>
      )}

      <div className="row gap-8" style={{ justifyContent: "flex-end" }}>
        <Button
          size="sm"
          variant="outline"
          disabled={testTunnel.isPending}
          onClick={onTestClick}
        >
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
          size="sm"
          variant="outline"
          disabled={
            rebuildTunnel.isPending && rebuildTunnel.variables === tunnel.id
          }
          onClick={() => rebuildTunnel.mutate(tunnel.id)}
        >
          {rebuildTunnel.isPending && rebuildTunnel.variables === tunnel.id ? (
            <>
              <Loader2 className="mr-1 size-3 animate-spin" aria-hidden />
              Rebuilding
            </>
          ) : (
            <>
              <RotateCcw className="mr-1 size-3" aria-hidden />
              Rebuild
            </>
          )}
        </Button>
        <Button
          size="sm"
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
    </div>
  );
}
