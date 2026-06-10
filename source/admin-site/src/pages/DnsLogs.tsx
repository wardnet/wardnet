import { useEffect, useMemo, useRef, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";

import { PageHeader } from "@/components/compound/PageHeader";
import { DataTable } from "@/components/core/ui/data-table";
import { Toggle } from "@wardnet/web";
import { Button } from "@wardnet/web";
import { Pill } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { DeviceSelect } from "@/components/compound/DeviceSelect";
import { useDevices } from "@wardnet/web";
import { useDnsQueryLog } from "@wardnet/web";
import { useDnsLogStore } from "@/stores/dnsLogStore";
import { formatTime } from "@wardnet/web";
import type {
  DnsQueryLogEntry,
  DnsQueryResult,
  QueryLogEvent,
} from "@wardnet/js";

interface RowShape {
  timestamp: string;
  client_ip: string;
  domain: string;
  query_type: string;
  result: string;
  latency_ms: number;
}

const RESULT_BADGE: Record<string, "ok" | "warn" | "down" | "info" | "ghost"> =
  {
    blocked: "down",
    // Suppressed by the per-device kill switch / global stop. Ghost tone
    // signals "would-have-blocked" without the alarming red.
    blocked_skipped: "ghost",
    upstream_error: "down",
    forwarded: "ghost",
    cache_hit: "ghost",
    rewritten: "info",
    recursive: "info",
    error: "warn",
  };

const RESULT_LABEL: Record<string, string> = {
  blocked_skipped: "blocked (skipped)",
};

function fmtTime(ts: string): string {
  // Persisted entries arrive as ISO `YYYY-MM-DDTHH:MM:SSZ`; live
  // events use the same wire format. Delegated to the shared
  // 24-hour formatter so log timestamps render identically across
  // every page that displays them.
  return formatTime(ts.endsWith("Z") ? ts : `${ts}Z`);
}

const PAGE_SIZE = 50;

/** DNS query log page: live tail + paginated history. Admin only. */
export default function DnsLogs() {
  const [domain, setDomain] = useState("");
  const [clientIp, setClientIp] = useState("");
  const [result, setResult] = useState("any");
  const [page, setPage] = useState(0);
  const [liveTail, setLiveTail] = useState(true);

  const { data: devicesData } = useDevices();
  // De-dupe by last_ip so the dropdown doesn't list two rows for the
  // same IP — the DNS log is keyed on `client_ip` and that's what the
  // filter narrows on.
  const filterableDevices = useMemo(() => {
    const seen = new Set<string>();
    return (devicesData?.devices ?? []).filter((d) => {
      if (!d.last_ip || seen.has(d.last_ip)) return false;
      seen.add(d.last_ip);
      return true;
    });
  }, [devicesData]);

  const liveEntries = useDnsLogStore((s) => s.entries);
  const liveConnected = useDnsLogStore((s) => s.connected);
  const setStorePaused = useDnsLogStore((s) => s.setPaused);
  const setStoreFilter = useDnsLogStore((s) => s.setFilter);

  // Wire local filter into the global store filter so the WS broadcast
  // narrows the live tail too.
  useEffect(() => {
    setStoreFilter({
      domain,
      client_ip: clientIp,
      results: result === "any" ? [] : [result],
    });
  }, [domain, clientIp, result, setStoreFilter]);

  // Auto-pause the store on filter change so a sudden flood of new
  // events doesn't interrupt scrolling.
  const userScrolledRef = useRef(false);
  useEffect(() => {
    setStorePaused(!liveTail);
  }, [liveTail, setStorePaused]);

  const filterParams = useMemo(
    () => ({
      limit: PAGE_SIZE,
      offset: page * PAGE_SIZE,
      domain: domain || undefined,
      client_ip: clientIp || undefined,
      result: result === "any" ? undefined : (result as DnsQueryResult),
    }),
    [domain, clientIp, result, page],
  );
  const { data, isLoading } = useDnsQueryLog(filterParams);

  const showLive = liveTail;
  const liveRows: RowShape[] = liveEntries
    .filter(matchesFilter(domain, clientIp, result))
    .map(eventToRow);
  const persistedRows: RowShape[] = data?.entries.map(persistedToRow) ?? [];

  const rows = showLive ? liveRows : persistedRows;
  const totalRows = data?.total ?? 0;

  const columns: ColumnDef<RowShape>[] = useMemo(
    () => [
      {
        accessorKey: "timestamp",
        header: "Time",
        meta: { className: "w-24" },
        cell: ({ row }) => (
          <span className="font-mono text-xs">
            {fmtTime(row.original.timestamp)}
          </span>
        ),
      },
      {
        accessorKey: "client_ip",
        header: "Device",
        meta: { className: "w-56" },
        cell: ({ row }) => {
          const dev = devicesData?.devices.find(
            (d) => d.last_ip === row.original.client_ip,
          );
          const primary = dev?.name || dev?.hostname || row.original.client_ip;
          const secondary =
            dev?.name || dev?.hostname ? row.original.client_ip : null;
          return (
            <div className="flex items-center gap-2">
              {dev && <DeviceIcon type={dev.device_type} size={16} />}
              <div className="flex min-w-0 flex-col">
                <span className="truncate font-medium">{primary}</span>
                {secondary && (
                  <span className="truncate text-xs text-ink-3">
                    {secondary}
                  </span>
                )}
              </div>
            </div>
          );
        },
      },
      {
        accessorKey: "domain",
        header: "Domain",
        cell: ({ row }) => (
          <span className="block truncate font-mono text-xs">
            {row.original.domain}
          </span>
        ),
      },
      {
        accessorKey: "query_type",
        header: "Type",
        meta: { className: "hidden w-16 sm:table-cell" },
      },
      {
        accessorKey: "result",
        header: "Result",
        meta: { className: "w-28" },
        cell: ({ row }) => (
          <Pill variant={RESULT_BADGE[row.original.result] ?? "ghost"}>
            {RESULT_LABEL[row.original.result] ?? row.original.result}
          </Pill>
        ),
      },
      {
        accessorKey: "latency_ms",
        header: "Latency",
        meta: { className: "hidden w-24 sm:table-cell" },
        cell: ({ row }) => (
          <span className="tabular-nums text-ink-3">
            {row.original.latency_ms.toFixed(1)} ms
          </span>
        ),
      },
    ],
    [devicesData],
  );

  // Toolbar filters — sit before the search input. Device + Result
  // dropdowns use the medium select variant so they match the search
  // height (34px). The search input itself is the Domain filter.
  const filters = (
    <>
      <DeviceSelect
        id="device-filter"
        devices={filterableDevices}
        value={clientIp}
        onChange={(ip) => {
          setClientIp(ip);
          setPage(0);
        }}
        triggerClassName="select-trigger--md w-48"
      />
      <Select
        value={result}
        onValueChange={(v) => {
          setResult(v);
          setPage(0);
        }}
      >
        <SelectTrigger className="select-trigger--md w-40">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="any">Any result</SelectItem>
          <SelectItem value="forwarded">Forwarded</SelectItem>
          <SelectItem value="blocked">Blocked</SelectItem>
          <SelectItem value="blocked_skipped">Blocked (skipped)</SelectItem>
          <SelectItem value="cache_hit">Cache hit</SelectItem>
          <SelectItem value="rewritten">Rewritten</SelectItem>
          <SelectItem value="upstream_error">Upstream error</SelectItem>
        </SelectContent>
      </Select>
    </>
  );

  // Live tail moves to the PageHeader: it's a viewing-mode choice
  // ("what shows up?"), not a query filter applied to results.
  const liveTailAction = (
    <label className="flex items-center gap-2 text-sm text-ink-3">
      <span>Live tail{liveConnected ? "" : " (offline)"}</span>
      <Toggle id="live-tail" checked={liveTail} onCheckedChange={setLiveTail} />
    </label>
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      <PageHeader
        title="DNS query log"
        description="Every DNS query passing through Wardnet, live or historical. Filter by device, result, or domain."
        actions={liveTailAction}
      />

      <DataTable
        columns={columns}
        data={rows}
        emptyMessage={
          isLoading ? "Loading…" : "No DNS queries match this filter yet."
        }
        fixedLayout
        searchValue={domain}
        onSearchChange={(v) => {
          setDomain(v);
          setPage(0);
          userScrolledRef.current = false;
        }}
        searchPlaceholder="Search domain…"
        filters={filters}
      />

      {!showLive && (
        <div className="flex shrink-0 items-center justify-between text-xs text-ink-3">
          <span>
            {totalRows.toLocaleString()} entries · page {page + 1}
          </span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={page === 0}
              onClick={() => setPage((p) => Math.max(0, p - 1))}
            >
              Previous
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={(page + 1) * PAGE_SIZE >= totalRows}
              onClick={() => setPage((p) => p + 1)}
            >
              Next
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

function matchesFilter(
  domain: string,
  clientIp: string,
  result: string,
): (e: QueryLogEvent) => boolean {
  return (e) => {
    if (domain && !e.domain.includes(domain)) return false;
    if (clientIp && e.client_ip !== clientIp) return false;
    if (result !== "any" && e.result !== result) return false;
    return true;
  };
}

function eventToRow(e: QueryLogEvent): RowShape {
  return {
    timestamp: e.timestamp,
    client_ip: e.client_ip,
    domain: e.domain,
    query_type: e.query_type,
    result: e.result,
    latency_ms: e.latency_ms,
  };
}

function persistedToRow(e: DnsQueryLogEntry): RowShape {
  return {
    timestamp: e.timestamp,
    client_ip: e.client_ip,
    domain: e.domain,
    query_type: e.query_type,
    result: e.result,
    latency_ms: e.latency_ms,
  };
}
