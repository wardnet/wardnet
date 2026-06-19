import { useMemo, type ReactNode } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Bookmark, Clock } from "lucide-react";
import { Pill } from "@wardnet/web";
import {
  DataTable,
  RowAction,
  type DataTableGroup,
} from "@/components/core/ui/data-table";
import { DeviceIcon } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { DiscoveryPlaceholder } from "@/components/compound/DiscoveryPlaceholder";
import { HostCell, buildDeviceIndex } from "@/components/compound/HostCell";
import { StatusBadge } from "@/components/compound/StatusBadge";
import type {
  Device,
  DhcpLease,
  DhcpLeaseStatus,
  DhcpReservation,
} from "@wardnet/js";

export type DhcpGroupId = "all" | "reservations" | "leases";

interface DhcpEntry {
  kind: "reservation" | "lease";
  /** Stable react key — namespaced so lease/reservation ids never clash. */
  rowKey: string;
  mac: string;
  ip: string;
  hostname: string | null;
  description: string | null;
  leaseStatus: DhcpLeaseStatus | null;
  lease: DhcpLease | null;
  reservation: DhcpReservation | null;
}

/** Kind + status as two separate pills. They stack vertically on
 *  mobile so the column doesn't push the row wider than the
 *  viewport, and sit inline at `md+` where there's room. */
function leaseTypeBadge(status: DhcpLeaseStatus): ReactNode {
  const label = status.charAt(0).toUpperCase() + status.slice(1);
  return (
    <div className="flex flex-col items-start gap-1 md:flex-row md:items-center md:gap-1.5">
      <Pill variant="ghost">Lease</Pill>
      <StatusBadge tone={status === "active" ? "success" : "neutral"}>
        {label}
      </StatusBadge>
    </div>
  );
}

function reservationTypeBadge(): ReactNode {
  // "Static" matches the DHCP-correct counterpart to "Lease" (a
  // static reservation vs a dynamic lease) and keeps the column
  // width budget balanced with the lease row's small pills.
  return <Pill variant="info">Static</Pill>;
}

function buildColumns(
  deviceIndex: Map<string, Device>,
): ColumnDef<DhcpEntry>[] {
  return [
    {
      id: "host",
      header: "Host",
      cell: ({ row }) => {
        const entry = row.original;
        const device = deviceIndex.get(entry.mac.toLowerCase());
        const primary =
          device?.name ??
          entry.description ??
          entry.hostname ??
          device?.hostname ??
          entry.mac;
        const secondary = primary === entry.mac ? null : entry.mac;
        const fallbackIcon =
          entry.kind === "reservation" ? (
            <Bookmark size={18} className="text-ink-3" />
          ) : (
            <Clock size={18} className="text-ink-3" />
          );
        const icon = device ? (
          <DeviceIcon type={device.device_type} />
        ) : (
          fallbackIcon
        );
        return (
          <HostCell
            primary={primary}
            secondary={secondary}
            icon={icon}
            // The IP column is hidden below md so we surface the
            // address inside the Host cell on mobile instead.
            extra={<div className="mac mono md:hidden">{entry.ip}</div>}
          />
        );
      },
    },
    {
      id: "ip",
      header: "IP",
      meta: { className: "hidden md:table-cell" },
      cell: ({ row }) => (
        <Text as="span" size="xs" className="font-mono">
          {row.original.ip}
        </Text>
      ),
    },
    {
      id: "type",
      header: "Type",
      cell: ({ row }) =>
        row.original.kind === "reservation"
          ? reservationTypeBadge()
          : leaseTypeBadge(row.original.leaseStatus ?? "active"),
    },
    {
      id: "description",
      header: "Description",
      meta: { className: "hidden md:table-cell" },
      cell: ({ row }) => (
        <span className="text-ink-3">{row.original.description ?? "—"}</span>
      ),
    },
  ];
}

interface DhcpEntryTableProps {
  leases: DhcpLease[];
  reservations: DhcpReservation[];
  devices: Device[];
  activeGroup: DhcpGroupId;
  onGroupChange: (g: DhcpGroupId) => void;
  searchValue: string;
  onSearchChange: (q: string) => void;
  onMakeStatic: (lease: DhcpLease) => void;
  onRevokeLease: (id: string) => void;
  onDeleteReservation: (id: string) => void;
  onAddReservation: () => void;
  /** Fired when a row whose MAC matches a known device is clicked.
   *  Rows for unmanaged / unknown MACs don't trigger this — the
   *  consumer typically navigates to the device detail page. */
  onDeviceClick?: (deviceId: string) => void;
}

/** Combined DHCP entries table — reservations and leases share one
 *  surface with a group filter (All / Reservations / Leases), a
 *  search field (MAC · hostname · IP), and an Add reservation CTA. */
export function DhcpEntryTable({
  leases,
  reservations,
  devices,
  activeGroup,
  onGroupChange,
  searchValue,
  onSearchChange,
  onMakeStatic,
  onRevokeLease,
  onDeleteReservation,
  onAddReservation,
  onDeviceClick,
}: DhcpEntryTableProps) {
  const deviceIndex = useMemo(() => buildDeviceIndex(devices), [devices]);

  const entries = useMemo<DhcpEntry[]>(() => {
    const fromReservations: DhcpEntry[] = reservations.map((r) => ({
      kind: "reservation",
      rowKey: `res-${r.id}`,
      mac: r.mac_address,
      ip: r.ip_address,
      hostname: r.hostname,
      description: r.description,
      leaseStatus: null,
      lease: null,
      reservation: r,
    }));
    const fromLeases: DhcpEntry[] = leases.map((l) => ({
      kind: "lease",
      rowKey: `lease-${l.id}`,
      mac: l.mac_address,
      ip: l.ip_address,
      hostname: l.hostname,
      description: null,
      leaseStatus: l.status,
      lease: l,
      reservation: null,
    }));
    return [...fromReservations, ...fromLeases];
  }, [leases, reservations]);

  const counts = useMemo(
    () => ({
      all: entries.length,
      reservations: reservations.length,
      leases: leases.length,
    }),
    [entries.length, reservations.length, leases.length],
  );

  const filtered = useMemo(() => {
    const byGroup = entries.filter((e) => {
      if (activeGroup === "reservations") return e.kind === "reservation";
      if (activeGroup === "leases") return e.kind === "lease";
      return true;
    });
    const q = searchValue.trim().toLowerCase();
    if (!q) return byGroup;
    return byGroup.filter(
      (e) =>
        e.mac.toLowerCase().includes(q) ||
        (e.hostname ?? "").toLowerCase().includes(q) ||
        e.ip.toLowerCase().includes(q),
    );
  }, [entries, activeGroup, searchValue]);

  const columns = useMemo(() => buildColumns(deviceIndex), [deviceIndex]);

  const groups: DataTableGroup[] = [
    { id: "all", label: "All", count: counts.all },
    { id: "reservations", label: "Reservations", count: counts.reservations },
    { id: "leases", label: "Leases", count: counts.leases },
  ];

  // Empty initial state — show the discovery placeholder while we
  // wait for the first lease/reservation.
  if (entries.length === 0) {
    return (
      <DiscoveryPlaceholder
        cols={4}
        message="Waiting for DHCP activity"
        hint="Leases and reservations will appear here as devices come online."
      />
    );
  }

  return (
    <DataTable
      columns={columns}
      data={filtered}
      groups={groups}
      activeGroup={activeGroup}
      onGroupChange={(id) => onGroupChange(id as DhcpGroupId)}
      searchValue={searchValue}
      onSearchChange={onSearchChange}
      searchPlaceholder="Search by MAC, hostname or IP"
      addLabel="Add reservation"
      onAdd={onAddReservation}
      onRowClick={
        onDeviceClick
          ? (entry) => {
              const device = deviceIndex.get(entry.mac.toLowerCase());
              if (device) onDeviceClick(device.id);
            }
          : undefined
      }
      emptyMessage="No DHCP entries match the current filter."
      rowActions={(entry) => {
        if (entry.kind === "reservation" && entry.reservation) {
          return (
            <RowAction
              onSelect={() => onDeleteReservation(entry.reservation!.id)}
              destructive
            >
              Delete
            </RowAction>
          );
        }
        if (
          entry.kind === "lease" &&
          entry.lease &&
          entry.leaseStatus === "active"
        ) {
          return (
            <>
              <RowAction onSelect={() => onMakeStatic(entry.lease!)}>
                Make static
              </RowAction>
              <RowAction
                onSelect={() => onRevokeLease(entry.lease!.id)}
                destructive
              >
                Revoke
              </RowAction>
            </>
          );
        }
        return null;
      }}
    />
  );
}
