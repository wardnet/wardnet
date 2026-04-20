import type { ColumnDef } from "@tanstack/react-table";
import { Button } from "@/components/core/ui/button";
import { DataTable } from "@/components/core/ui/data-table";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { timeAgo } from "@/lib/utils";
import type { AllowlistEntry } from "@wardnet/js";

function createColumns(onDelete: (id: string) => void): ColumnDef<AllowlistEntry>[] {
  return [
    {
      accessorKey: "domain",
      header: "Domain",
      cell: ({ row }) => <span className="font-mono text-sm">{row.original.domain}</span>,
    },
    {
      accessorKey: "reason",
      header: "Reason",
      meta: { className: "hidden sm:table-cell" },
      cell: ({ row }) => (
        <span className="text-muted-foreground">{row.original.reason ?? "\u2014"}</span>
      ),
    },
    {
      accessorKey: "created_at",
      header: "Added",
      meta: { className: "hidden md:table-cell" },
      cell: ({ row }) => (
        <span className="text-muted-foreground">{timeAgo(row.original.created_at)}</span>
      ),
    },
    {
      id: "actions",
      header: "",
      meta: { className: "text-right" },
      cell: ({ row }) => (
        <Button variant="ghost" size="sm" onClick={() => onDelete(row.original.id)}>
          Remove
        </Button>
      ),
    },
  ];
}

interface AllowlistTableProps {
  entries: AllowlistEntry[];
  onDelete: (id: string) => void;
  onAdd: () => void;
}

/** Table listing allowlist entries with remove action. */
export function AllowlistTable({ entries, onDelete, onAdd }: AllowlistTableProps) {
  const columns = createColumns(onDelete);

  if (entries.length === 0) {
    return (
      <EmptyStatePlaceholder
        message="No allowlist entries"
        hint="Domains added here will never be blocked, even if they appear in a blocklist."
        actionLabel="Add Domain"
        onAction={onAdd}
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end">
        <Button size="sm" onClick={onAdd}>
          Add Domain
        </Button>
      </div>

      <DataTable columns={columns} data={entries} />
    </div>
  );
}
