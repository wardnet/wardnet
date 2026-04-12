import type { ColumnDef } from "@tanstack/react-table";
import { Button } from "@/components/core/ui/button";
import { DataTable } from "@/components/core/ui/data-table";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import type { DhcpReservation } from "@wardnet/js";

function createColumns(onDelete: (id: string) => void): ColumnDef<DhcpReservation>[] {
  return [
    {
      accessorKey: "ip_address",
      header: "IP",
      cell: ({ row }) => (
        <span className="font-mono text-xs">{row.original.ip_address}</span>
      ),
    },
    {
      accessorKey: "mac_address",
      header: "MAC",
      cell: ({ row }) => (
        <span className="font-mono text-xs text-muted-foreground">{row.original.mac_address}</span>
      ),
    },
    {
      accessorKey: "description",
      header: "Description",
      meta: { className: "hidden md:table-cell" },
      cell: ({ row }) => (
        <span className="text-muted-foreground">{row.original.description ?? "\u2014"}</span>
      ),
    },
    {
      id: "actions",
      header: "",
      meta: { className: "text-right" },
      cell: ({ row }) => (
        <Button variant="ghost" size="sm" onClick={() => onDelete(row.original.id)}>
          Delete
        </Button>
      ),
    },
  ];
}

interface DhcpReservationTableProps {
  reservations: DhcpReservation[];
  onDelete: (id: string) => void;
  onAdd: () => void;
}

/** Table listing DHCP reservations with delete action and add button. */
export function DhcpReservationTable({ reservations, onDelete, onAdd }: DhcpReservationTableProps) {
  const columns = createColumns(onDelete);

  if (reservations.length === 0) {
    return (
      <EmptyStatePlaceholder
        message="No DHCP reservations"
        hint="Add your first reservation to assign a permanent IP address to a device."
        actionLabel="Add Reservation"
        onAction={onAdd}
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end">
        <Button size="sm" onClick={onAdd}>
          Add Reservation
        </Button>
      </div>

      <DataTable columns={columns} data={reservations} />
    </div>
  );
}
