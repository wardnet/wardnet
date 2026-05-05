import { useMemo } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Bookmark } from "lucide-react";
import { Button } from "@/components/core/ui/button";
import { DataTable } from "@/components/core/ui/data-table";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { HostCell, buildDeviceIndex } from "@/components/compound/HostCell";
import { cn } from "@/lib/utils";
import type { Device, DhcpReservation } from "@wardnet/js";

function createColumns(
  deviceIndex: Map<string, Device>,
  onDelete: (id: string) => void,
): ColumnDef<DhcpReservation>[] {
  return [
    {
      accessorKey: "host",
      header: "Host",
      cell: ({ row }) => {
        const reservation = row.original;
        const device = deviceIndex.get(reservation.mac_address.toLowerCase());
        // Reservation hostname (admin-set, pushed to the client) is used as
        // the primary identifier when no device record or description is
        // available — better than falling all the way through to MAC.
        const primary =
          device?.name ??
          reservation.description ??
          reservation.hostname ??
          device?.hostname ??
          reservation.mac_address;
        const secondary = primary === reservation.mac_address ? null : reservation.mac_address;
        // Known devices reuse the device-table icon; truly unknown MACs
        // (no device record yet) fall back to a generic reservation icon.
        const icon = device ? (
          <DeviceIcon type={device.device_type} />
        ) : (
          <Bookmark size={18} className={cn("text-muted-foreground")} />
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
      accessorKey: "description",
      header: "Description",
      meta: { className: "hidden md:table-cell" },
      cell: ({ row }) => (
        <span className="text-muted-foreground">{row.original.description ?? "—"}</span>
      ),
    },
    {
      id: "actions",
      header: "",
      meta: { className: "text-right" },
      cell: ({ row }) => (
        <Button
          variant="tertiary"
          className="text-destructive hover:text-destructive"
          onClick={() => onDelete(row.original.id)}
        >
          Delete
        </Button>
      ),
    },
  ];
}

interface DhcpReservationTableProps {
  reservations: DhcpReservation[];
  devices: Device[];
  onDelete: (id: string) => void;
  onAdd: () => void;
}

/** Table listing DHCP reservations with delete action and add button. */
export function DhcpReservationTable({
  reservations,
  devices,
  onDelete,
  onAdd,
}: DhcpReservationTableProps) {
  const deviceIndex = useMemo(() => buildDeviceIndex(devices), [devices]);
  const columns = useMemo(() => createColumns(deviceIndex, onDelete), [deviceIndex, onDelete]);

  if (reservations.length === 0) {
    return (
      <EmptyStatePlaceholder
        message="No DHCP reservations"
        hint="Add your first reservation to assign a permanent IP address to a device."
        actionLabel="Add reservation"
        onAction={onAdd}
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end">
        <Button onClick={onAdd}>Add reservation</Button>
      </div>

      <DataTable columns={columns} data={reservations} />
    </div>
  );
}
