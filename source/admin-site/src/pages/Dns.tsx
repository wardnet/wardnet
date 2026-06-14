import { useState } from "react";
import { Link } from "react-router";
import {
  Card,
  CardAction,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Pill } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Button } from "@wardnet/web";
import { PageHeader } from "@/components/compound/PageHeader";
import { DashboardUsageBar } from "@/components/compound/DashboardUsageBar";
import { UpstreamServersCard } from "@/components/features/UpstreamServersCard";
import { SecuritySettingsCard } from "@/components/features/SecuritySettingsCard";
import { DnsStatsSection } from "@/components/features/DnsStatsSection";
import { Tabs, TabsList, TabsTrigger } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import {
  useDnsStatus,
  useDnsConfig,
  useToggleDns,
  useFlushDnsCache,
  useUpdateDnsConfig,
} from "@wardnet/web";
import { RANGES, type StatsRange } from "@wardnet/web";

/** DNS server configuration page (admin only). */
export default function Dns() {
  const { data: statusData, isLoading: statusLoading } = useDnsStatus();
  const { data: configData } = useDnsConfig();

  const toggleDns = useToggleDns();
  const flushCache = useFlushDnsCache();
  const updateConfig = useUpdateDnsConfig();

  const status = statusData;
  const config = configData?.config;

  // Retention edit-mode state — follows DhcpConfigCard pattern. The
  // draft is reset when leaving edit mode so subsequent edits start
  // from the latest server value.
  const [range, setRange] = useState<StatsRange>("24h");
  const [editingRetention, setEditingRetention] = useState(false);
  const [retentionDraft, setRetentionDraft] = useState<number>(7);

  function startRetentionEdit() {
    setRetentionDraft(config?.query_log_retention_days ?? 7);
    setEditingRetention(true);
  }
  function cancelRetentionEdit() {
    setEditingRetention(false);
  }
  function saveRetention() {
    updateConfig.mutate({ query_log_retention_days: retentionDraft });
    setEditingRetention(false);
  }

  const cacheUsagePercent =
    status && status.cache_capacity > 0
      ? (status.cache_size / status.cache_capacity) * 100
      : 0;

  return (
    <div className="col gap-20">
      <PageHeader
        title="DNS"
        description="Wardnet's local resolver: cache, upstream forwarders, and the query log."
      />

      {statusLoading && (
        <Card>
          <CardContent className="py-10 text-center text-ink-3">
            Loading DNS status...
          </CardContent>
        </Card>
      )}

      {status && config && (
        <div className="col gap-20">
          {/* Status & Cache cards */}
          <div className="grid gap-4 sm:grid-cols-2">
            {/* DNS server status — matches DhcpStatusCard chrome:
                title · pill · toggle in CardAction. */}
            <Card>
              <CardHeader>
                <CardTitle>DNS server</CardTitle>
                <Pill variant={status.running ? "ok" : "ghost"}>
                  <span className="dot" />
                  {status.running ? "Running" : "Stopped"}
                </Pill>
                <CardAction>
                  <Toggle
                    id="dns-toggle"
                    aria-label="Enable DNS"
                    checked={status.enabled}
                    onCheckedChange={(enabled) => toggleDns.mutate(enabled)}
                    disabled={toggleDns.isPending}
                  />
                </CardAction>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <div className="stat__label">Resolution mode</div>
                    <Select
                      value={config.resolution_mode}
                      onValueChange={(value) =>
                        updateConfig.mutate({ resolution_mode: value })
                      }
                    >
                      <SelectTrigger
                        className="mt-1"
                        aria-label="Resolution mode"
                      >
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="forwarding">Forwarding</SelectItem>
                        <SelectItem value="recursive">Recursive</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div>
                    <div className="stat__label">DNSSEC</div>
                    <div className="text-lg font-semibold">
                      {config.dnssec_enabled ? "Enabled" : "Disabled"}
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>

            {/* Cache card */}
            <Card>
              <CardHeader>
                <CardTitle>Cache</CardTitle>
                <CardAction>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => flushCache.mutate()}
                    disabled={flushCache.isPending}
                  >
                    {flushCache.isPending ? "Flushing…" : "Flush cache"}
                  </Button>
                </CardAction>
              </CardHeader>
              <CardContent>
                <div className="flex flex-col gap-4">
                  <div className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                      <div className="stat__label">Entries</div>
                      <div className="text-2xl font-bold">
                        {status.cache_size}
                      </div>
                    </div>
                    <div>
                      <div className="stat__label">Hit rate</div>
                      <div className="text-2xl font-bold">
                        {status.cache_hit_rate.toFixed(1)}%
                      </div>
                    </div>
                  </div>
                  <div>
                    <p className="mb-1 text-xs text-ink-3">
                      Cache usage ({status.cache_size} / {status.cache_capacity}
                      )
                    </p>
                    <DashboardUsageBar value={cacheUsagePercent} />
                  </div>
                </div>
              </CardContent>
            </Card>
          </div>

          {/* Security settings — DNSSEC, rebinding protection, rate limit
              (Stage 4). */}
          <SecuritySettingsCard />

          {/* Query log card — Pill + Toggle in header (matches DHCP
              status card); retention follows the Edit/Save pattern
              from DhcpConfigCard. */}
          <Card>
            <CardHeader>
              <CardTitle>Query log</CardTitle>
              <Pill variant={config.query_log_enabled ? "ok" : "ghost"}>
                <span className="dot" />
                {config.query_log_enabled ? "Enabled" : "Disabled"}
              </Pill>
              <CardAction className="flex gap-2">
                {!editingRetention && config.query_log_enabled && (
                  <>
                    <Button asChild variant="outline" size="sm">
                      <Link to="/dns/logs">View DNS log</Link>
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={startRetentionEdit}
                      disabled={updateConfig.isPending}
                    >
                      Edit
                    </Button>
                  </>
                )}
                <Toggle
                  id="query-log-toggle"
                  aria-label="Enable query log retention"
                  checked={config.query_log_enabled}
                  onCheckedChange={(enabled) =>
                    updateConfig.mutate({ query_log_enabled: enabled })
                  }
                  disabled={updateConfig.isPending}
                />
              </CardAction>
            </CardHeader>

            {editingRetention ? (
              <>
                <CardContent>
                  <Field
                    label="Retention (days)"
                    htmlFor="retention-days"
                    help="Approx. disk at 5 qps household traffic: 1d ≈ 50 MB, 7d ≈ 350 MB, 30d ≈ 1.5 GB."
                  >
                    <Input
                      id="retention-days"
                      type="number"
                      min={1}
                      max={30}
                      value={retentionDraft}
                      onChange={(e) =>
                        setRetentionDraft(Number(e.target.value))
                      }
                      className="w-28"
                    />
                  </Field>
                </CardContent>
                <CardFooter className="justify-end gap-2">
                  <Button
                    variant="ghost"
                    onClick={cancelRetentionEdit}
                    disabled={updateConfig.isPending}
                  >
                    Cancel
                  </Button>
                  <Button
                    onClick={saveRetention}
                    disabled={
                      updateConfig.isPending ||
                      retentionDraft === config.query_log_retention_days ||
                      retentionDraft < 1 ||
                      retentionDraft > 30
                    }
                  >
                    {updateConfig.isPending ? "Saving…" : "Save"}
                  </Button>
                </CardFooter>
              </>
            ) : (
              <CardContent>
                <div>
                  <div className="stat__label">Retention</div>
                  <div className="text-lg font-semibold">
                    {config.query_log_enabled
                      ? `${config.query_log_retention_days} days`
                      : "—"}
                  </div>
                </div>
              </CardContent>
            )}
          </Card>

          {/* Upstream servers — own card with data-table + row actions
              + inline add form. */}
          <UpstreamServersCard
            servers={config.upstream_servers}
            isSaving={updateConfig.isPending}
            fallbackOnly={config.resolution_mode === "recursive"}
            onUpdate={(servers) =>
              updateConfig.mutate({ upstream_servers: servers })
            }
          />

          {/* DNS query stats — range tabs above the section; state lifted
              here because the range controls cards, chart, and top lists. */}
          <div className="col gap-4">
            <div className="flex justify-end">
              <Tabs
                value={range}
                onValueChange={(v) => setRange(v as StatsRange)}
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
            <DnsStatsSection range={range} />
          </div>
        </div>
      )}
    </div>
  );
}
