import type { ColumnDef } from "@tanstack/react-table";
import { DataTable, RowAction } from "@/components/core/ui/data-table";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { timeAgo } from "@/lib/utils";
import type { AllowlistEntry } from "@wardnet/js";

function createColumns(): ColumnDef<AllowlistEntry>[] {
  return [
    {
      accessorKey: "domain",
      header: "Domain",
      cell: ({ row }) => (
        <span className="block truncate font-mono" title={row.original.domain}>
          {row.original.domain}
        </span>
      ),
    },
    {
      accessorKey: "reason",
      header: "Reason",
      meta: { className: "hidden sm:table-cell" },
      cell: ({ row }) => (
        <span className="block truncate text-ink-3" title={row.original.reason ?? ""}>
          {row.original.reason ?? "—"}
        </span>
      ),
    },
    {
      accessorKey: "created_at",
      header: "Added",
      meta: { className: "hidden w-32 md:table-cell" },
      cell: ({ row }) => <span className="text-ink-3">{timeAgo(row.original.created_at)}</span>,
    },
  ];
}

interface AllowlistTableProps {
  entries: AllowlistEntry[];
  onDelete: (id: string) => void;
  onAdd: () => void;
}

/** Table listing allowlist entries. Per-row remove via the overflow
 *  menu; an outline-sm CTA in the toolbar opens the add panel. */
export function AllowlistTable({ entries, onDelete, onAdd }: AllowlistTableProps) {
  const columns = createColumns();

  if (entries.length === 0) {
    return (
      <EmptyStatePlaceholder
        message="No allowlist entries"
        hint="Domains added here will never be blocked, even if they appear in a blocklist."
        actionLabel="Add domain"
        onAction={onAdd}
      />
    );
  }

  return (
    <DataTable
      columns={columns}
      data={entries}
      fixedLayout
      addLabel="Add domain"
      onAdd={onAdd}
      rowActions={(entry) => (
        <RowAction onSelect={() => onDelete(entry.id)} destructive>
          Remove
        </RowAction>
      )}
    />
  );
}
