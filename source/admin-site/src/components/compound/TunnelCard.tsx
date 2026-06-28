import {
  ArrowDown,
  ArrowUp,
  Gauge,
  Loader2,
  RotateCcw,
  SlidersHorizontal,
} from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { StatusBadge } from "./StatusBadge";
import {
  countryFlag,
  formatBytes,
  formatMbps,
  formatMs,
  retentionPct,
  timeAgo,
} from "@wardnet/web";
import {
  useTestTunnel,
  useRebuildTunnel,
  useSpeedTestResults,
  useStartSpeedTest,
} from "@wardnet/web";
import type {
  Tunnel,
  TunnelStatus,
  ProviderInfo,
  TunnelTestResult,
  TunnelSpeedTestResult,
} from "@wardnet/js";

/**
 * Direct-vs-tunnel comparison grid. A raw tunnel number is meaningless on its
 * own, so each row shows the direct (WAN) baseline beside the tunnel value
 * plus the delta — the only honest read on VPN quality.
 */
function SpeedTestComparison({ result }: { result: TunnelSpeedTestResult }) {
  const kept = retentionPct(
    result.direct_throughput_mbps,
    result.tunnel_throughput_mbps,
  );
  const latencyOverhead = result.tunnel_latency_ms - result.direct_latency_ms;
  const jitterOverhead = result.tunnel_jitter_ms - result.direct_jitter_ms;

  return (
    <div
      data-testid="tunnel-speed-test-result"
      className="col gap-4 rounded-[var(--radius-md)] border border-[var(--line)] bg-[var(--bg-sunken)] px-3 py-2 text-xs"
    >
      <div className="row text-ink-3">
        <Text as="span" size="2xs" weight="medium">
          Speed test
        </Text>
        <span className="ml-auto">tested {timeAgo(result.tested_at)}</span>
      </div>
      <div
        className="grid items-center gap-x-3 gap-y-1"
        style={{ gridTemplateColumns: "auto 1fr 1fr auto" }}
      >
        <span className="text-ink-3" />
        <Text as="span" size="2xs" weight="medium" className="text-ink-3">
          Direct
        </Text>
        <Text as="span" size="2xs" weight="medium" className="text-ink-3">
          Tunnel
        </Text>
        <span className="text-ink-3" />

        <span className="text-ink-3">Download</span>
        <span className="mono">
          {formatMbps(result.direct_throughput_mbps)}
        </span>
        <span className="mono" data-testid="tunnel-speed-test-download">
          {formatMbps(result.tunnel_throughput_mbps)}
        </span>
        <Text
          as="span"
          weight="medium"
          className={kept >= 80 ? "text-[var(--success-ink)]" : "text-ink-2"}
        >
          {kept}% kept
        </Text>

        <span className="text-ink-3">Latency</span>
        <span className="mono">{formatMs(result.direct_latency_ms)}</span>
        <span className="mono" data-testid="tunnel-speed-test-latency">
          {formatMs(result.tunnel_latency_ms)}
        </span>
        <span className="text-ink-2">
          {latencyOverhead >= 0 ? "+" : ""}
          {formatMs(latencyOverhead)}
        </span>

        <span className="text-ink-3">Jitter</span>
        <span className="mono">{formatMs(result.direct_jitter_ms)}</span>
        <span className="mono">{formatMs(result.tunnel_jitter_ms)}</span>
        <span className="text-ink-2">
          {jitterOverhead >= 0 ? "+" : ""}
          {formatMs(jitterOverhead)}
        </span>
      </div>
    </div>
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
  const startSpeedTest = useStartSpeedTest();
  // Only fetch this tunnel's speed-test history once the user has run a test
  // on this card (avoids one request per card across the whole tunnels grid on
  // load — persisted history lives on the tunnel detail page). A run in flight
  // also enables it so the prior result shows while the new test runs.
  const [speedTestRequested, setSpeedTestRequested] = useState(false);
  const speedTestRunning =
    startSpeedTest.isRunning && startSpeedTest.activeTunnelId === tunnel.id;
  const speedTestResults = useSpeedTestResults(
    tunnel.id,
    speedTestRequested || speedTestRunning,
  );
  const latestSpeedTest = speedTestResults.data?.results[0] ?? null;
  const [testResult, setTestResult] = useState<TunnelTestResult | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const [testedAt, setTestedAt] = useState<string | null>(null);

  const onSpeedTestClick = (e: React.MouseEvent) => {
    // Card is wrapped in <Link>; don't navigate when clicking Speed test.
    e.preventDefault();
    e.stopPropagation();
    setSpeedTestRequested(true);
    startSpeedTest.start(tunnel.id);
  };

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
              <Text
                as="span"
                size="2xs"
                weight="medium"
                className="rounded-sm bg-sunken px-1.5 py-0.5 text-ink-3"
                aria-label={`Auto-selected best server in ${tunnel.server_selector.country.toUpperCase()}`}
              >
                Best {tunnel.server_selector.country.toUpperCase()}
              </Text>
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
        <Text
          as="div"
          size="xs"
          className="row gap-8 rounded-[var(--radius-md)] border border-[var(--line)] bg-[var(--bg-sunken)] px-3 py-2"
        >
          <span aria-hidden>{countryFlag(testResult.country_code)}</span>
          <Text as="span" weight="medium">
            {testResult.country_code.toUpperCase()}
          </Text>
          <span className="text-ink-3">·</span>
          <span className="mono">{testResult.exit_ip}</span>
          <span className="text-ink-3">·</span>
          <span className="mono">{testResult.latency_ms} ms</span>
          {testedAt && (
            <span className="ml-auto text-ink-3">
              tested {timeAgo(testedAt)}
            </span>
          )}
        </Text>
      )}
      {testError && !testResult && (
        <div
          role="alert"
          className="rounded-[var(--radius-md)] border border-[var(--danger-soft)] bg-[var(--danger-soft)] px-3 py-2 text-xs text-[var(--danger-soft-ink)]"
        >
          {testError}
        </div>
      )}

      {latestSpeedTest && <SpeedTestComparison result={latestSpeedTest} />}

      <div className="row gap-8" style={{ justifyContent: "flex-end" }}>
        <Button
          size="sm"
          variant="outline"
          data-testid="tunnel-speed-test-button"
          disabled={speedTestRunning}
          onClick={onSpeedTestClick}
        >
          {speedTestRunning ? (
            <>
              <Loader2 className="mr-1 size-3 animate-spin" aria-hidden />
              Testing {startSpeedTest.percentage}%
            </>
          ) : (
            <>
              <Gauge className="mr-1 size-3" aria-hidden />
              Speed test
            </>
          )}
        </Button>
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
