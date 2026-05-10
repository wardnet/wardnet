import { useMemo } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Clock } from "lucide-react";
import { Button } from "@/components/core/ui/button";
import { DataTable } from "@/components/core/ui/data-table";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { DiscoveryPlaceholder } from "@/components/compound/DiscoveryPlaceholder";
import { HostCell, buildDeviceIndex } from "@/components/compound/HostCell";
import { StatusBadge } from "@/components/compound/StatusBadge";
import { cn, timeAgo } from "@/lib/utils";
import type { Device, DhcpLease, DhcpLeaseStatus } from "@wardnet/js";

function leaseStatusTone(status: DhcpLeaseStatus): "success" | "neutral" {
  return status === "active" ? "success" : "neutral";
}

function leaseStatusLabel(status: DhcpLeaseStatus): string {
  return status.charAt(0).toUpperCase() + status.slice(1);
}

function createColumns(
  deviceIndex: Map<string, Device>,
  onRevoke: (id: string) => void,
  onMakeStatic?: (lease: DhcpLease) => void,
): ColumnDef<DhcpLease>[] {
  return [
    {
      accessorKey: "host",
      header: "Host",
      cell: ({ row }) => {
        const lease = row.original;
        const device = deviceIndex.get(lease.mac_address);
        const primary = device?.name ?? lease.hostname ?? device?.hostname ?? lease.mac_address;
        const secondary = primary === lease.mac_address ? null : lease.mac_address;
        // Known devices reuse the device-table icon for visual consistency;
        // truly unknown MACs (no device record yet) fall back to a generic
        // lease icon.
        const icon = device ? (
          <DeviceIcon type={device.device_type} />
        ) : (
          <Clock size={18} className={cn("text-muted-foreground")} />
        );
        return <HostCell primary={primary} secondary={secondary} icon={icon} />;
      },
    },
    {
      accessorKey: "ip_address",
      header: "IP",
      cell: ({ row }) => <span className="font-mono text-xs">{row.original.ip_address}</span>,
    },
    {
      accessorKey: "status",
      header: "Status",
      cell: ({ row }) => (
        <StatusBadge tone={leaseStatusTone(row.original.status)}>
          {leaseStatusLabel(row.original.status)}
        </StatusBadge>
      ),
    },
    {
      accessorKey: "lease_end",
      header: "Expires",
      meta: { className: "hidden lg:table-cell" },
      cell: ({ row }) => (
        <span className="text-muted-foreground">{timeAgo(row.original.lease_end)}</span>
      ),
    },
    {
      id: "actions",
      header: "",
      meta: { className: "text-right" },
      cell: ({ row }) =>
        row.original.status === "active" ? (
          <div className="flex justify-end gap-4">
            {onMakeStatic && (
              <Button variant="tertiary" onClick={() => onMakeStatic(row.original)}>
                Make static
              </Button>
            )}
            <Button
              variant="tertiary"
              className="text-destructive hover:text-destructive"
              onClick={() => onRevoke(row.original.id)}
            >
              Revoke
            </Button>
          </div>
        ) : null,
    },
  ];
}

interface DhcpLeaseTableProps {
  leases: DhcpLease[];
  devices: Device[];
  onRevoke: (id: string) => void;
  onMakeStatic?: (lease: DhcpLease) => void;
}

/** Table listing DHCP leases with status badges, revoke, and make-static actions. */
export function DhcpLeaseTable({ leases, devices, onRevoke, onMakeStatic }: DhcpLeaseTableProps) {
  const deviceIndex = useMemo(() => buildDeviceIndex(devices), [devices]);
  const columns = useMemo(
    () => createColumns(deviceIndex, onRevoke, onMakeStatic),
    [deviceIndex, onRevoke, onMakeStatic],
  );

  if (leases.length === 0) {
    return (
      <DiscoveryPlaceholder
        cols={5}
        message="Waiting for DHCP leases"
        hint="Leases will appear when devices receive addresses from the DHCP server."
      />
    );
  }

  return <DataTable columns={columns} data={leases} />;
}
