import { useState } from "react";
import { GlobeIcon, ShieldIcon, Trash2Icon, Loader2Icon } from "lucide-react";
import { Pill, Text, Heading } from "@wardnet/web";
import { Sparkline } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import {
  useDnsStatus,
  useDnsConfig,
  useToggleDns,
  useUpdateDnsConfig,
  useFlushDnsCache,
  useDashboardDnsStats,
  useDnsTopBlockedDomains,
  parseLabels,
} from "@wardnet/web";
import { useOnlineStatusContext } from "@/context/OnlineStatusContext";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { SectionLabel } from "@/components/SectionLabel";
import { Bar } from "@/components/Bar";

export default function Dns() {
  const { data: status, isLoading: statusLoading } = useDnsStatus();
  const { data: configData, isLoading: configLoading } = useDnsConfig();
  const { data: dnsStats, isLoading: statsLoading } = useDashboardDnsStats();
  const { data: topDomains, isLoading: topDomainsLoading } =
    useDnsTopBlockedDomains(10);
  const toggleDns = useToggleDns();
  const updateConfig = useUpdateDnsConfig();
  const flushCache = useFlushDnsCache();
  const { showingLastKnownState } = useOnlineStatusContext();

  const [dnsToggleConfirmOpen, setDnsToggleConfirmOpen] = useState(false);
  const [filterToggleConfirmOpen, setFilterToggleConfirmOpen] = useState(false);

  const isLoading =
    statusLoading || configLoading || statsLoading || topDomainsLoading;
  const config = configData?.config;
  const dnsEnabled = config?.enabled ?? false;
  const filteringEnabled = config?.dns_filtering_enabled ?? false;
  const isRunning = status?.running ?? false;
  const cacheSize = status?.cache_size ?? 0;
  const cacheCapacity = status?.cache_capacity ?? 0;
  const cacheHitRate = status?.cache_hit_rate ?? 0;
  const topEntries = topDomains?.entries ?? [];

  if (isLoading) {
    return (
      <div className="flex flex-col gap-5 p-4">
        <div>
          <div className="h-8 w-24 animate-pulse rounded-lg bg-sunken" />
          <div className="mt-1 h-4 w-48 animate-pulse rounded bg-sunken" />
        </div>
        <div className="h-52 animate-pulse rounded-xl bg-sunken" />
        <div className="h-28 animate-pulse rounded-xl bg-sunken" />
        <div className="h-48 animate-pulse rounded-xl bg-sunken" />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-5 p-4">
      {/* Page header */}
      <div>
        <Heading level={1} size="3xl" weight="bold" className="text-ink">
          DNS
        </Heading>
        <Text as="p" size="base" className="text-ink-3">
          Server status, stats, and filtering.
        </Text>
      </div>

      <div
        className={
          showingLastKnownState
            ? "pointer-events-none opacity-40 transition-opacity"
            : "transition-opacity"
        }
      >
        <div className="flex flex-col gap-5">
          {/* ── Status card ── */}
          <div>
            <SectionLabel>Status</SectionLabel>
            <div className="rounded-xl border border-line bg-card p-4">
              {/* Running state row */}
              <div className="flex items-center gap-2.5">
                <span
                  className={`size-2 shrink-0 rounded-full ${isRunning ? "bg-accent" : "bg-warn"}`}
                />
                <Text
                  as="span"
                  size="lg"
                  weight="semibold"
                  className="text-ink"
                >
                  DNS Server
                </Text>
                <div className="ml-auto">
                  <Pill
                    variant={isRunning ? "ok" : "down"}
                    data-testid="dns-status-pill"
                  >
                    <span className="mr-1" aria-hidden>
                      ●
                    </span>
                    {isRunning ? "Running" : "Stopped"}
                  </Pill>
                </div>
              </div>

              {/* Stats + cache metric grid */}
              <div className="mt-4 grid grid-cols-2 gap-x-4 gap-y-4 border-t border-line pt-4">
                <div>
                  <Text
                    as="p"
                    size="2xs"
                    weight="semibold"
                    className="uppercase tracking-wider text-ink-3"
                  >
                    Queries · 24h
                  </Text>
                  <Text
                    as="p"
                    size="base"
                    className="mt-1 text-ink tabular-nums"
                  >
                    {dnsStats?.total.toLocaleString() ?? "-"}
                  </Text>
                  {dnsStats && dnsStats.totalSeries.length > 0 && (
                    <div className="mt-1.5 h-6">
                      <Sparkline
                        values={dnsStats.totalSeries}
                        color="var(--color-ink-3)"
                        area={false}
                      />
                    </div>
                  )}
                </div>
                <div>
                  <Text
                    as="p"
                    size="2xs"
                    weight="semibold"
                    className="uppercase tracking-wider text-ink-3"
                  >
                    Blocked · 24h
                  </Text>
                  <Text
                    as="p"
                    size="base"
                    className="mt-1 text-ink tabular-nums"
                  >
                    {dnsStats ? `${dnsStats.blockedPercent.toFixed(1)}%` : "-"}
                  </Text>
                  {dnsStats && dnsStats.blockedSeries.length > 0 && (
                    <div className="mt-1.5 h-6">
                      <Sparkline
                        values={dnsStats.blockedSeries}
                        color="var(--color-warn)"
                        area={false}
                      />
                    </div>
                  )}
                </div>
                <div>
                  <Text
                    as="p"
                    size="2xs"
                    weight="semibold"
                    className="uppercase tracking-wider text-ink-3"
                  >
                    Cache
                  </Text>
                  <Text
                    as="p"
                    size="base"
                    className="mt-1 text-ink tabular-nums"
                  >
                    {cacheSize.toLocaleString()} /{" "}
                    {cacheCapacity.toLocaleString()}
                  </Text>
                </div>
                <div>
                  <Text
                    as="p"
                    size="2xs"
                    weight="semibold"
                    className="uppercase tracking-wider text-ink-3"
                  >
                    Hit Rate
                  </Text>
                  <Text
                    as="p"
                    size="base"
                    className="mt-1 text-ink tabular-nums"
                  >
                    {(cacheHitRate * 100).toFixed(1)}%
                  </Text>
                  <Bar percent={cacheHitRate * 100} variant="rate" />
                </div>
              </div>

              {/* Flush cache action */}
              <div className="mt-4 flex justify-end border-t border-line pt-4">
                <button
                  data-testid="dns-flush-cache"
                  onClick={() => flushCache.mutate()}
                  disabled={flushCache.isPending}
                  className="flex items-center gap-1.5 text-ink-3 disabled:opacity-40 active:text-ink"
                >
                  {flushCache.isPending ? (
                    <Loader2Icon size={13} className="animate-spin" />
                  ) : (
                    <Trash2Icon size={13} />
                  )}
                  <Text as="span" size="sm" weight="medium">
                    Flush DNS cache
                  </Text>
                </button>
              </div>
            </div>
          </div>

          {/* ── Controls section — split cards ── */}
          <div>
            <SectionLabel>Controls</SectionLabel>
            <div className="flex flex-col gap-2">
              <div className="flex items-center gap-3 rounded-xl border border-line bg-card px-4 py-3.5">
                <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-sunken">
                  <GlobeIcon size={18} className="text-ink-2" />
                </div>
                <div className="min-w-0 flex-1">
                  <Text
                    as="p"
                    size="base"
                    weight="semibold"
                    className="text-ink"
                  >
                    DNS Server
                  </Text>
                  <Text as="p" size="xs" className="text-ink-3">
                    Enable or disable the DNS resolver
                  </Text>
                </div>
                <Toggle
                  checked={dnsEnabled}
                  onCheckedChange={() => setDnsToggleConfirmOpen(true)}
                  disabled={toggleDns.isPending}
                  aria-label="Toggle DNS server"
                  data-testid="dns-toggle"
                />
              </div>

              <div className="flex items-center gap-3 rounded-xl border border-line bg-card px-4 py-3.5">
                <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-sunken">
                  <ShieldIcon size={18} className="text-ink-2" />
                </div>
                <div className="min-w-0 flex-1">
                  <Text
                    as="p"
                    size="base"
                    weight="semibold"
                    className="text-ink"
                  >
                    DNS Filtering
                  </Text>
                  <Text as="p" size="xs" className="text-ink-3">
                    Global emergency stop for all filtering
                  </Text>
                </div>
                <Toggle
                  checked={filteringEnabled}
                  onCheckedChange={() => setFilterToggleConfirmOpen(true)}
                  disabled={updateConfig.isPending}
                  aria-label="Toggle DNS filtering"
                  data-testid="dns-filter-toggle"
                />
              </div>
            </div>
          </div>

          {/* ── Top blocked domains ── */}
          <div>
            <SectionLabel>Top Blocked Domains · 24h</SectionLabel>
            <div
              data-testid="dns-top-blocked"
              className="rounded-xl border border-line bg-card"
            >
              {topEntries.length === 0 ? (
                <Text as="p" size="sm" className="py-8 text-center text-ink-3">
                  No blocked queries in the last 24 hours.
                </Text>
              ) : (
                <ol className="divide-y divide-line">
                  {topEntries.map((entry, i) => {
                    const domain =
                      parseLabels(entry.labels).domain ?? entry.labels;
                    return (
                      <li
                        key={domain ?? String(i)}
                        className="flex items-center gap-3 px-4 py-3"
                      >
                        <Text
                          as="span"
                          size="xs"
                          className="w-5 shrink-0 text-right text-ink-4 tabular-nums"
                        >
                          {i + 1}
                        </Text>
                        <Text
                          as="span"
                          size="sm"
                          className="min-w-0 flex-1 truncate font-mono text-ink"
                        >
                          {domain}
                        </Text>
                        <Text
                          as="span"
                          size="sm"
                          className="shrink-0 text-ink-3 tabular-nums"
                        >
                          {entry.total.toLocaleString()}
                        </Text>
                      </li>
                    );
                  })}
                </ol>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Confirm dialogs */}
      <ConfirmDialog
        open={dnsToggleConfirmOpen}
        onOpenChange={setDnsToggleConfirmOpen}
        onConfirm={() => {
          toggleDns.mutate(!dnsEnabled);
          setDnsToggleConfirmOpen(false);
        }}
        title={dnsEnabled ? "Disable DNS server?" : "Enable DNS server?"}
        description={
          dnsEnabled
            ? "All devices on the network will lose DNS resolution until the server is re-enabled."
            : "The DNS server will start resolving queries for all devices."
        }
        confirmLabel={dnsEnabled ? "Disable" : "Enable"}
        variant={dnsEnabled ? "danger" : "warn"}
      />
      <ConfirmDialog
        open={filterToggleConfirmOpen}
        onOpenChange={setFilterToggleConfirmOpen}
        onConfirm={() => {
          updateConfig.mutate({ dns_filtering_enabled: !filteringEnabled });
          setFilterToggleConfirmOpen(false);
        }}
        title={
          filteringEnabled ? "Disable DNS filtering?" : "Enable DNS filtering?"
        }
        description={
          filteringEnabled
            ? "All filtering will be bypassed for every device. Blocklists and custom rules will have no effect."
            : "DNS filtering will resume for all devices using their configured profiles."
        }
        confirmLabel={
          filteringEnabled ? "Disable filtering" : "Enable filtering"
        }
        variant={filteringEnabled ? "danger" : "warn"}
      />
    </div>
  );
}
