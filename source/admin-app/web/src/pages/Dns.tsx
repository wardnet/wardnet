import { useState } from "react";
import { Link } from "react-router";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Pill } from "@wardnet/forge-web/pill";
import { Toggle } from "@wardnet/forge-web/toggle";
import { Field } from "@wardnet/forge-web/field";
import { Input } from "@wardnet/forge-web/input";
import { Button } from "@wardnet/forge-web/button";
import { PageHeader } from "@/components/compound/PageHeader";
import { DashboardUsageBar } from "@/components/compound/DashboardUsageBar";
import { StatusBadge } from "@/components/compound/StatusBadge";
import {
  useDnsStatus,
  useDnsConfig,
  useToggleDns,
  useFlushDnsCache,
  useUpdateDnsConfig,
} from "@/hooks/useDns";

/** DNS server configuration page (admin only). */
export default function Dns() {
  const { data: statusData, isLoading: statusLoading } = useDnsStatus();
  const { data: configData } = useDnsConfig();

  const toggleDns = useToggleDns();
  const flushCache = useFlushDnsCache();
  const updateConfig = useUpdateDnsConfig();

  const status = statusData;
  const config = configData?.config;

  // Local "draft" for the retention input. Falls back to the persisted
  // value while the user hasn't typed anything, so the field always
  // reflects the latest server state without an effect-driven sync.
  const [retentionDraft, setRetentionDraft] = useState<number | null>(null);
  const retentionDays = retentionDraft ?? config?.query_log_retention_days ?? 7;

  const cacheUsagePercent =
    status && status.cache_capacity > 0 ? (status.cache_size / status.cache_capacity) * 100 : 0;

  return (
    <>
      <PageHeader title="DNS" />

      {statusLoading && (
        <Card>
          <CardContent className="py-10 text-center text-ink-3">Loading DNS status...</CardContent>
        </Card>
      )}

      {status && config && (
        <div className="flex min-h-0 flex-1 flex-col gap-4">
          {/* Status & Cache cards */}
          <div className="grid gap-4 sm:grid-cols-2">
            {/* Status card */}
            <Card>
              <CardHeader className="flex flex-row items-center justify-between">
                <CardTitle className="text-sm font-medium text-ink-3">DNS server</CardTitle>
                <StatusBadge
                  tone={status.running ? "success" : "neutral"}
                  withIcon={status.running}
                >
                  {status.running ? "Running" : "Stopped"}
                </StatusBadge>
              </CardHeader>
              <CardContent>
                <div className="flex flex-col gap-4">
                  <Field direction="row" label="Enable DNS" htmlFor="dns-toggle">
                    <Toggle
                      id="dns-toggle"
                      checked={status.enabled}
                      onCheckedChange={(enabled) => toggleDns.mutate(enabled)}
                      disabled={toggleDns.isPending}
                    />
                  </Field>
                  <div className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                      <p className="text-ink-3">Resolution mode</p>
                      <p className="text-lg font-semibold capitalize">{config.resolution_mode}</p>
                    </div>
                    <div>
                      <p className="text-ink-3">DNSSEC</p>
                      <p className="text-lg font-semibold">
                        {config.dnssec_enabled ? "Enabled" : "Disabled"}
                      </p>
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>

            {/* Cache card */}
            <Card>
              <CardHeader className="flex flex-row items-center justify-between">
                <CardTitle className="text-sm font-medium text-ink-3">Cache</CardTitle>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => flushCache.mutate()}
                  disabled={flushCache.isPending}
                >
                  {flushCache.isPending ? "Flushing…" : "Flush cache"}
                </Button>
              </CardHeader>
              <CardContent>
                <div className="flex flex-col gap-4">
                  <div className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                      <p className="text-ink-3">Entries</p>
                      <p className="text-2xl font-bold">{status.cache_size}</p>
                    </div>
                    <div>
                      <p className="text-ink-3">Hit rate</p>
                      <p className="text-2xl font-bold">{status.cache_hit_rate.toFixed(1)}%</p>
                    </div>
                  </div>
                  <div>
                    <p className="mb-1 text-xs text-ink-3">
                      Cache usage ({status.cache_size} / {status.cache_capacity})
                    </p>
                    <DashboardUsageBar value={cacheUsagePercent} />
                  </div>
                </div>
              </CardContent>
            </Card>
          </div>

          {/* Query log card */}
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <CardTitle className="text-sm font-medium text-ink-3">Query log</CardTitle>
              <Button asChild variant="outline" size="sm">
                <Link to="/dns/logs">View DNS log →</Link>
              </Button>
            </CardHeader>
            <CardContent>
              <div className="flex flex-col gap-4">
                <Field
                  direction="row"
                  label="Retain DNS query log"
                  htmlFor="query-log-toggle"
                  help="Live stream remains available regardless."
                >
                  <Toggle
                    id="query-log-toggle"
                    checked={config.query_log_enabled}
                    onCheckedChange={(enabled) =>
                      updateConfig.mutate({ query_log_enabled: enabled })
                    }
                    disabled={updateConfig.isPending}
                  />
                </Field>
                <Field
                  label="Retention (days)"
                  htmlFor="retention-days"
                  help="Approx. disk at 5 qps household traffic: 1d ≈ 50 MB, 7d ≈ 350 MB, 30d ≈ 1.5 GB."
                >
                  <div className="flex items-end gap-2">
                    <Input
                      id="retention-days"
                      type="number"
                      min={1}
                      max={30}
                      value={retentionDays}
                      onChange={(e) => setRetentionDraft(Number(e.target.value))}
                      className="w-28"
                      disabled={!config.query_log_enabled}
                    />
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={
                        updateConfig.isPending ||
                        !config.query_log_enabled ||
                        retentionDays === config.query_log_retention_days ||
                        retentionDays < 1 ||
                        retentionDays > 30
                      }
                      onClick={() => {
                        updateConfig.mutate({ query_log_retention_days: retentionDays });
                        setRetentionDraft(null);
                      }}
                    >
                      Save
                    </Button>
                  </div>
                </Field>
              </div>
            </CardContent>
          </Card>

          {/* Upstream servers card */}
          <Card>
            <CardHeader>
              <CardTitle className="text-sm font-medium text-ink-3">Upstream servers</CardTitle>
            </CardHeader>
            <CardContent>
              {config.upstream_servers.length === 0 ? (
                <p className="text-sm text-ink-3">No upstream servers configured.</p>
              ) : (
                <div className="divide-y">
                  {config.upstream_servers.map((server, i) => (
                    <div
                      key={i}
                      className="flex items-center justify-between py-3 first:pt-0 last:pb-0"
                    >
                      <div className="flex flex-col gap-0.5">
                        <p className="text-sm font-medium">{server.name}</p>
                        <p className="text-xs text-ink-3">
                          {server.address}
                          {server.port ? `:${server.port}` : ""}
                        </p>
                      </div>
                      <Pill variant="ghost">{server.protocol.toUpperCase()}</Pill>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      )}
    </>
  );
}
