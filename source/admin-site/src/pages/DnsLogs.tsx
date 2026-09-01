import { useEffect, useMemo, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";

import { PageHeader } from "@/components/compound/PageHeader";
import { DataTable } from "@/components/core/ui/data-table";
import { Toggle } from "@wardnet/web";
import { Button } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Pill } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import { HelpCircle } from "lucide-react";
import { DeviceIcon } from "@wardnet/web";
import { DeviceSelect } from "@/components/compound/DeviceSelect";
import { HostCell } from "@/components/compound/HostCell";
import { useDevices, deviceDisplayName } from "@wardnet/web";
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
  /** Write-time device attribution; null for unknown sources and rows
   *  recorded before attribution existed. */
  device_id: string | null;
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
    // Successful resolution whose answer is "no such name / record type"
    // (NXDOMAIN / NODATA) — informational, not an error.
    negative: "info",
    // Negative answers served from a local authoritative zone — successful
    // resolutions whose answer is "no such name / record type".
    authoritative_nodata: "info",
    authoritative_nxdomain: "info",
    // Refused: the answer pointed at a private IP for an external domain.
    rebinding_blocked: "down",
    rate_limited: "warn",
    recursor_failed: "warn",
    forwarded: "ghost",
    cache_hit: "ghost",
    rewritten: "info",
    recursive: "info",
    error: "warn",
  };

// Keep these short. The Result column is fixed-width and the Pill is
// `white-space: nowrap`, so a label wider than the column overflows into
// Latency rather than wrapping or truncating.
const RESULT_LABEL: Record<string, string> = {
  blocked_skipped: "blocked (skipped)",
  negative: "no record",
  // Distinct from `negative`: answered by a local authoritative zone, not
  // upstream. Same meaning, different source — the labels must differ or the
  // two are indistinguishable in the table.
  authoritative_nodata: "no record (local)",
  authoritative_nxdomain: "no such name (local)",
  rebinding_blocked: "rebinding",
  rate_limited: "rate limited",
  recursor_failed: "recursor",
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
  const [deviceId, setDeviceId] = useState("");
  const [clientIp, setClientIp] = useState("");
  const [result, setResult] = useState("any");
  const [page, setPage] = useState(0);
  const [liveTail, setLiveTail] = useState(true);

  const { data: devicesData } = useDevices();
  // The dropdown is id-keyed so duplicates by IP are fine, but a device
  // with no name, no hostname, and an empty last_ip would render a blank
  // option (DeviceSelect's label falls back name → hostname → last_ip).
  const filterableDevices = useMemo(
    () =>
      (devicesData?.devices ?? []).filter(
        (d) => d.name || d.hostname || d.last_ip,
      ),
    [devicesData],
  );

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
      device_id: deviceId,
      results: result === "any" ? [] : [result],
    });
  }, [domain, clientIp, deviceId, result, setStoreFilter]);

  // Auto-pause the store when live tail is off so a sudden flood of new
  // events doesn't interrupt scrolling.
  useEffect(() => {
    setStorePaused(!liveTail);
  }, [liveTail, setStorePaused]);

  const filterParams = useMemo(
    () => ({
      limit: PAGE_SIZE,
      offset: page * PAGE_SIZE,
      domain: domain || undefined,
      client_ip: clientIp || undefined,
      device_id: deviceId || undefined,
      result: result === "any" ? undefined : (result as DnsQueryResult),
    }),
    [domain, clientIp, deviceId, result, page],
  );
  const { data, isLoading } = useDnsQueryLog(filterParams);

  const showLive = liveTail;
  const liveRows: RowShape[] = liveEntries
    .filter(matchesFilter(domain, clientIp, deviceId, result))
    .map(eventToRow);
  const persistedRows: RowShape[] = data?.entries.map(persistedToRow) ?? [];

  const rows = showLive ? liveRows : persistedRows;
  const hasMore = data?.has_more ?? false;
  // The endpoint returns no total — a count over dns_query_log is a full scan.
  // The range is derived from the page offset and the rows actually returned,
  // so a short last page reads "Showing 51–63" and gives back the exact size
  // of a result set narrow enough to end within the page.
  const rangeStart = persistedRows.length === 0 ? 0 : page * PAGE_SIZE + 1;
  const rangeEnd = page * PAGE_SIZE + persistedRows.length;

  const columns: ColumnDef<RowShape>[] = useMemo(
    () => [
      {
        accessorKey: "timestamp",
        header: "Time",
        meta: { className: "w-24" },
        cell: ({ row }) => (
          <Text size="xs" className="font-mono">
            {fmtTime(row.original.timestamp)}
          </Text>
        ),
      },
      {
        accessorKey: "client_ip",
        header: "Device",
        meta: { className: "w-56" },
        cell: ({ row }) => {
          // Resolve by the row's write-time device attribution — never by
          // IP, which DHCP may have reassigned since the query happened.
          const dev = row.original.device_id
            ? devicesData?.devices.find((d) => d.id === row.original.device_id)
            : undefined;
          return (
            <HostCell
              primary={dev ? deviceDisplayName(dev) : row.original.client_ip}
              secondary={dev ? row.original.client_ip : null}
              icon={
                dev ? (
                  <DeviceIcon type={dev.device_type} size={16} />
                ) : (
                  <HelpCircle size={16} className="text-ink-3" />
                )
              }
            />
          );
        },
      },
      {
        accessorKey: "domain",
        header: "Domain",
        cell: ({ row }) => (
          <Text size="xs" className="block truncate font-mono">
            {row.original.domain}
          </Text>
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
      {/* Device and client-IP are one question ("which client?") asked two
          ways, so they're wrapped rather than left as toolbar siblings: the
          toolbar is `flex-wrap`, and loose siblings let the IP box wrap onto
          its own row away from the dropdown it belongs with.

          The pair stays on one row at every width instead of being allowed to
          wrap, so the controls narrow on small screens rather than pushing the
          toolbar into horizontal scroll: `min-w-0` defeats the default
          `min-width:auto` that would otherwise stop a flex item shrinking
          below its content. At 360px the group is ~284px and fits. */}
      <div className="flex min-w-0 items-center gap-3">
        <DeviceSelect
          id="device-filter"
          devices={filterableDevices}
          valueKey="id"
          value={deviceId}
          onChange={(id) => {
            setDeviceId(id);
            setPage(0);
          }}
          triggerClassName="select-trigger--md w-40 min-w-0 sm:w-48"
        />
        {/* Free-text IP filter: the only handle on rows with no device
            attribution (unknown sources, pre-attribution history). */}
        <Input
          id="client-ip-filter"
          data-testid="dns-log-ip-filter"
          className="w-28 min-w-0 sm:w-36"
          placeholder="Client IP…"
          value={clientIp}
          onChange={(e) => {
            setClientIp(e.target.value);
            setPage(0);
          }}
        />
      </div>
      <Select
        value={result}
        onValueChange={(v) => {
          setResult(v);
          setPage(0);
        }}
      >
        <SelectTrigger
          className="select-trigger--md w-40"
          data-testid="dns-log-result-filter"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="any">Any result</SelectItem>
          <SelectItem value="forwarded">Forwarded</SelectItem>
          <SelectItem value="blocked">Blocked</SelectItem>
          <SelectItem value="blocked_skipped">Blocked (skipped)</SelectItem>
          <SelectItem value="cache_hit">Cache hit</SelectItem>
          <SelectItem value="rewritten">Rewritten</SelectItem>
          <SelectItem value="negative">Negative (no record)</SelectItem>
          <SelectItem value="authoritative">Authoritative</SelectItem>
          {/* The filter is an exact `result = ?` match server-side, so every
              result string needs its own option — otherwise rows the table
              renders are unreachable through the filter. */}
          <SelectItem value="authoritative_nodata">
            Authoritative (no record)
          </SelectItem>
          <SelectItem value="authoritative_nxdomain">
            Authoritative (no such name)
          </SelectItem>
          <SelectItem value="rate_limited">Rate limited</SelectItem>
          <SelectItem value="rebinding_blocked">Rebinding blocked</SelectItem>
          <SelectItem value="recursive">Recursive</SelectItem>
          <SelectItem value="recursor_failed">Recursor failed</SelectItem>
          <SelectItem value="upstream_error">Upstream error</SelectItem>
          <SelectItem value="error">Error</SelectItem>
        </SelectContent>
      </Select>
    </>
  );

  // Live tail moves to the PageHeader: it's a viewing-mode choice
  // ("what shows up?"), not a query filter applied to results.
  const liveTailAction = (
    <Text as="label" size="sm" className="flex items-center gap-2 text-ink-3">
      <span>Live tail{liveConnected ? "" : " (offline)"}</span>
      <Toggle
        id="live-tail"
        data-testid="live-tail"
        checked={liveTail}
        onCheckedChange={setLiveTail}
      />
    </Text>
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
        }}
        searchPlaceholder="Search domain…"
        filters={filters}
      />

      {!showLive && (
        <Text
          as="div"
          size="xs"
          className="flex shrink-0 items-center justify-between text-ink-3"
        >
          <span>
            {rangeEnd === 0
              ? "No entries"
              : `Showing ${rangeStart.toLocaleString()}–${rangeEnd.toLocaleString()} · page ${page + 1}`}
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
              disabled={!hasMore}
              onClick={() => setPage((p) => p + 1)}
            >
              Next
            </Button>
          </div>
        </Text>
      )}
    </div>
  );
}

function matchesFilter(
  domain: string,
  clientIp: string,
  deviceId: string,
  result: string,
): (e: QueryLogEvent) => boolean {
  return (e) => {
    if (domain && !e.domain.includes(domain)) return false;
    // Substring match, mirroring the domain filter and the server-side LIKE.
    if (clientIp && !e.client_ip.includes(clientIp)) return false;
    if (deviceId && e.device_id !== deviceId) return false;
    if (result !== "any" && e.result !== result) return false;
    return true;
  };
}

function eventToRow(e: QueryLogEvent): RowShape {
  return {
    timestamp: e.timestamp,
    client_ip: e.client_ip,
    device_id: e.device_id ?? null,
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
    device_id: e.device_id ?? null,
    domain: e.domain,
    query_type: e.query_type,
    result: e.result,
    latency_ms: e.latency_ms,
  };
}
