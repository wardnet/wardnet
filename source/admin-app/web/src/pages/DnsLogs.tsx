import { useEffect, useMemo, useRef, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";

import { PageHeader } from "@/components/compound/PageHeader";
import { DataTable } from "@/components/core/ui/data-table";
import { Card, CardContent } from "@/components/core/ui/card";
import { Input } from "@/components/core/ui/input";
import { Switch } from "@/components/core/ui/switch";
import { Label } from "@/components/core/ui/label";
import { Button } from "@wardnet/forge-web/button";
import { Badge } from "@/components/core/ui/badge";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { DeviceSelect } from "@/components/compound/DeviceSelect";
import { useDevices } from "@/hooks/useDevices";
import { useDnsQueryLog } from "@/hooks/useDnsLogs";
import { useDnsLogStore } from "@/stores/dnsLogStore";
import type { DnsQueryLogEntry, QueryLogEvent } from "@wardnet/js";

interface RowShape {
  timestamp: string;
  client_ip: string;
  domain: string;
  query_type: string;
  result: string;
  latency_ms: number;
}

const RESULT_BADGE: Record<string, "default" | "destructive" | "secondary" | "outline"> = {
  blocked: "destructive",
  // Suppressed by the per-device kill switch / global stop. Outline tone
  // signals "would-have-blocked" without the alarming red.
  blocked_skipped: "outline",
  upstream_error: "destructive",
  forwarded: "secondary",
  cache_hit: "outline",
  cached: "outline",
  rewritten: "default",
  local: "default",
  recursive: "default",
};

const RESULT_LABEL: Record<string, string> = {
  blocked_skipped: "blocked (skipped)",
};

function fmtTime(ts: string): string {
  // The persisted entries arrive as ISO `YYYY-MM-DDTHH:MM:SSZ`. Live
  // events use the same wire format.
  const d = new Date(ts.endsWith("Z") ? ts : `${ts}Z`);
  if (Number.isNaN(d.getTime())) return ts;
  return d.toLocaleTimeString();
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
      result: result === "any" ? undefined : result,
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
        // Fixed widths via per-column className keep the layout stable
        // when rows change — without this, `<table>`'s default auto
        // layout re-fits column widths to content on every filter
        // change, which makes the table flicker.
        meta: { className: "w-24" },
        cell: ({ row }) => (
          <span className="font-mono text-xs">{fmtTime(row.original.timestamp)}</span>
        ),
      },
      {
        accessorKey: "client_ip",
        header: "Device",
        meta: { className: "w-56" },
        cell: ({ row }) => {
          const dev = devicesData?.devices.find((d) => d.last_ip === row.original.client_ip);
          const primary = dev?.name || dev?.hostname || row.original.client_ip;
          const secondary = dev?.name || dev?.hostname ? row.original.client_ip : null;
          return (
            <div className="flex items-center gap-2">
              {dev && <DeviceIcon type={dev.device_type} size={16} />}
              <div className="flex min-w-0 flex-col">
                <span className="truncate font-medium">{primary}</span>
                {secondary && (
                  <span className="truncate text-xs text-muted-foreground">{secondary}</span>
                )}
              </div>
            </div>
          );
        },
      },
      {
        accessorKey: "domain",
        header: "Domain",
        // Domain takes the remaining space and truncates long names.
        cell: ({ row }) => (
          <span className="block truncate font-mono text-xs">{row.original.domain}</span>
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
          <Badge variant={RESULT_BADGE[row.original.result] ?? "secondary"}>
            {RESULT_LABEL[row.original.result] ?? row.original.result}
          </Badge>
        ),
      },
      {
        accessorKey: "latency_ms",
        header: "Latency",
        meta: { className: "hidden w-24 sm:table-cell" },
        cell: ({ row }) => (
          <span className="tabular-nums text-muted-foreground">
            {row.original.latency_ms.toFixed(1)} ms
          </span>
        ),
      },
    ],
    [devicesData],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title="DNS query log" />

      <Card className="mb-4 shrink-0">
        <CardContent className="flex flex-col gap-3 p-4 sm:flex-row sm:items-end sm:gap-4">
          {/* Filters share a flex group with a max-width so on wide
              screens they don't stretch across the row — leaving the
              live tail toggle space to sit at the right edge with a
              comfortable gap. Inside the group each input gets its
              own flex weight: device is wider since it packs icon +
              two-line label; domain and result are equal. */}
          <div className="flex flex-col gap-3 sm:flex-1 sm:flex-row sm:items-end sm:gap-3 sm:max-w-3xl">
            <div className="flex-1">
              <Label className="text-xs text-muted-foreground">Domain</Label>
              <Input
                placeholder="example.com"
                value={domain}
                onChange={(e) => {
                  setDomain(e.target.value);
                  setPage(0);
                  userScrolledRef.current = false;
                }}
              />
            </div>
            <div className="flex-[2]">
              <Label htmlFor="device-filter" className="text-xs text-muted-foreground">
                Device
              </Label>
              <DeviceSelect
                id="device-filter"
                devices={filterableDevices}
                value={clientIp}
                onChange={(ip) => {
                  setClientIp(ip);
                  setPage(0);
                }}
              />
            </div>
            <div className="flex-1">
              <Label className="text-xs text-muted-foreground">Result</Label>
              <Select
                value={result}
                onValueChange={(v) => {
                  setResult(v);
                  setPage(0);
                }}
              >
                <SelectTrigger className="w-full">
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
            </div>
          </div>
          <div className="flex items-center gap-2 sm:ml-auto">
            <Switch id="live-tail" checked={liveTail} onCheckedChange={setLiveTail} />
            <Label htmlFor="live-tail" className="text-sm">
              Live tail{liveConnected ? "" : " (offline)"}
            </Label>
          </div>
        </CardContent>
      </Card>

      {/* Inner scroll container — gives the table its own scroll context
          so the filter card above stays pinned at the top. The shared
          DataTable's <th> sticky-positioning pins to the top of THIS
          container (the nearest non-`visible` overflow ancestor). */}
      <div className="min-h-0 flex-1 overflow-y-auto">
        <DataTable
          columns={columns}
          data={rows}
          emptyMessage={isLoading ? "Loading…" : "No DNS queries match this filter yet."}
          fixedLayout
        />
      </div>

      {!showLive && (
        <div className="mt-3 flex shrink-0 items-center justify-between text-xs text-muted-foreground">
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
