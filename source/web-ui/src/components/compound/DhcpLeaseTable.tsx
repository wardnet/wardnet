import type { ColumnDef } from "@tanstack/react-table";
import { Badge } from "@/components/core/ui/badge";
import { Button } from "@/components/core/ui/button";
import { DataTable } from "@/components/core/ui/data-table";
import { DiscoveryPlaceholder } from "@/components/compound/DiscoveryPlaceholder";
import { timeAgo } from "@/lib/utils";
import type { DhcpLease, DhcpLeaseStatus } from "@wardnet/js";

function leaseStatusVariant(status: DhcpLeaseStatus) {
  const map = {
    active: "default",
    expired: "secondary",
    released: "outline",
  } as const;
  return map[status];
}

function createColumns(
  onRevoke: (id: string) => void,
  onMakeStatic?: (lease: DhcpLease) => void,
): ColumnDef<DhcpLease>[] {
  return [
    {
      accessorKey: "ip_address",
      header: "IP",
      cell: ({ row }) => <span className="font-mono text-xs">{row.original.ip_address}</span>,
    },
    {
      accessorKey: "mac_address",
      header: "MAC",
      meta: { className: "hidden sm:table-cell" },
      cell: ({ row }) => (
        <span className="font-mono text-xs text-muted-foreground">{row.original.mac_address}</span>
      ),
    },
    {
      accessorKey: "hostname",
      header: "Hostname",
      meta: { className: "hidden md:table-cell" },
      cell: ({ row }) => (
        <span className="text-muted-foreground">{row.original.hostname ?? "\u2014"}</span>
      ),
    },
    {
      accessorKey: "status",
      header: "Status",
      cell: ({ row }) => (
        <Badge variant={leaseStatusVariant(row.original.status)}>{row.original.status}</Badge>
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
          <div className="flex justify-end gap-1">
            {onMakeStatic && (
              <Button variant="ghost" size="sm" onClick={() => onMakeStatic(row.original)}>
                Make Static
              </Button>
            )}
            <Button variant="ghost" size="sm" onClick={() => onRevoke(row.original.id)}>
              Revoke
            </Button>
          </div>
        ) : null,
    },
  ];
}

interface DhcpLeaseTableProps {
  leases: DhcpLease[];
  onRevoke: (id: string) => void;
  onMakeStatic?: (lease: DhcpLease) => void;
}

/** Table listing DHCP leases with status badges, revoke, and make-static actions. */
export function DhcpLeaseTable({ leases, onRevoke, onMakeStatic }: DhcpLeaseTableProps) {
  const columns = createColumns(onRevoke, onMakeStatic);

  if (leases.length === 0) {
    return (
      <DiscoveryPlaceholder
        cols={6}
        message="Waiting for DHCP leases"
        hint="Leases will appear when devices receive addresses from the DHCP server."
      />
    );
  }

  return <DataTable columns={columns} data={leases} />;
}
