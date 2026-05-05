import type { ColumnDef } from "@tanstack/react-table";
import { Badge } from "@/components/core/ui/badge";
import { DataTable } from "@/components/core/ui/data-table";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { StatusBadge } from "@/components/compound/StatusBadge";
import { deviceTypeLabel } from "@/lib/device";
import { timeAgo } from "@/lib/utils";
import type { Device, DhcpStatus } from "@wardnet/js";

const columns: ColumnDef<Device>[] = [
  {
    accessorKey: "name",
    header: "Device",
    cell: ({ row }) => {
      const device = row.original;
      return (
        <div className="flex items-center gap-3">
          <DeviceIcon type={device.device_type} />
          <div className="flex flex-col">
            <span className="font-medium">{device.name ?? device.hostname ?? device.mac}</span>
            {(device.name || device.hostname) && (
              <span className="text-xs text-muted-foreground">{device.mac}</span>
            )}
          </div>
        </div>
      );
    },
  },
  {
    accessorKey: "last_ip",
    header: "IP",
    meta: { className: "hidden md:table-cell" },
    cell: ({ row }) => <span className="text-muted-foreground">{row.original.last_ip}</span>,
  },
  {
    accessorKey: "device_type",
    header: "Type",
    meta: { className: "hidden sm:table-cell" },
    cell: ({ row }) => (
      <Badge variant="secondary">{deviceTypeLabel(row.original.device_type)}</Badge>
    ),
  },
  {
    accessorKey: "manufacturer",
    header: "Manufacturer",
    meta: { className: "hidden lg:table-cell" },
    cell: ({ row }) => (
      <span className="text-muted-foreground">{row.original.manufacturer ?? "\u2014"}</span>
    ),
  },
  {
    accessorKey: "dhcp_status",
    header: "DHCP",
    meta: { className: "hidden xl:table-cell" },
    cell: ({ row }) => {
      const status: DhcpStatus = row.original.dhcp_status;
      switch (status) {
        case "lease":
          return <StatusBadge tone="success">Lease</StatusBadge>;
        case "reservation":
          return <StatusBadge tone="success">Reserved</StatusBadge>;
        case "external":
          return <StatusBadge tone="neutral">External</StatusBadge>;
      }
    },
  },
  {
    accessorKey: "last_seen",
    header: "Last seen",
    meta: { className: "text-right" },
    cell: ({ row }) => (
      <span className="text-muted-foreground">{timeAgo(row.original.last_seen)}</span>
    ),
  },
];

interface DeviceTableProps {
  devices: Device[];
  onDeviceClick: (deviceId: string) => void;
}

/** Table listing network devices. Receives pre-filtered, pre-sorted data. */
export function DeviceTable({ devices, onDeviceClick }: DeviceTableProps) {
  return (
    <DataTable columns={columns} data={devices} onRowClick={(device) => onDeviceClick(device.id)} />
  );
}
