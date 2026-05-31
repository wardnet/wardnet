import type { ColumnDef } from "@tanstack/react-table";
import { DataTable, RowAction } from "@/components/core/ui/data-table";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { StatusBadge } from "@/components/compound/StatusBadge";
import { timeAgo } from "@/lib/utils";
import type { Blocklist } from "@wardnet/js";

function createColumns(): ColumnDef<Blocklist>[] {
  return [
    {
      accessorKey: "name",
      header: "Name",
      // No explicit width — this column takes the remaining horizontal
      // space; long URLs truncate via the inner spans (see fixedLayout
      // note on the DataTable below).
      cell: ({ row }) => (
        <div className="col min-w-0 gap-0.5">
          <span className="truncate font-medium">{row.original.name}</span>
          <span className="mono truncate text-xs text-ink-3" title={row.original.url}>
            {row.original.url}
          </span>
          {row.original.last_error && (
            <span className="truncate text-xs text-danger" title={row.original.last_error}>
              {row.original.last_error}
            </span>
          )}
        </div>
      ),
    },
    {
      accessorKey: "entry_count",
      header: "Entries",
      meta: { className: "hidden w-24 sm:table-cell" },
      cell: ({ row }) => <span className="mono">{row.original.entry_count.toLocaleString()}</span>,
    },
    {
      accessorKey: "last_updated",
      header: "Updated at",
      meta: { className: "hidden w-32 md:table-cell" },
      cell: ({ row }) => (
        <span className="text-ink-3">
          {row.original.last_updated ? timeAgo(row.original.last_updated) : "Never"}
        </span>
      ),
    },
    {
      accessorKey: "enabled",
      header: "Status",
      meta: { className: "w-28" },
      cell: ({ row }) => (
        <StatusBadge tone={row.original.enabled ? "success" : "neutral"}>
          {row.original.enabled ? "Enabled" : "Disabled"}
        </StatusBadge>
      ),
    },
  ];
}

interface BlocklistTableProps {
  blocklists: Blocklist[];
  onRefresh: (id: string) => void;
  onToggle: (blocklist: Blocklist) => void;
  onEdit: (blocklist: Blocklist) => void;
  onDelete: (id: string) => void;
  refreshingId?: string | null;
  onAdd: () => void;
}

/** Table listing blocklists. Row click opens the edit sheet; the
 *  overflow `…` menu hosts toggle, refresh, and delete actions. */
export function BlocklistTable({
  blocklists,
  onRefresh,
  onToggle,
  onEdit,
  onDelete,
  refreshingId = null,
  onAdd,
}: BlocklistTableProps) {
  const columns = createColumns();

  if (blocklists.length === 0) {
    return (
      <EmptyStatePlaceholder
        message="No blocklists configured"
        hint="Add a blocklist URL to start blocking ads and trackers network-wide."
        actionLabel="Add blocklist"
        onAction={onAdd}
      />
    );
  }

  return (
    // fixedLayout pins the auxiliary column widths so the Name column
    // absorbs all remaining width; long URLs truncate inside it.
    <DataTable
      columns={columns}
      data={blocklists}
      onRowClick={onEdit}
      fixedLayout
      addLabel="Add blocklist"
      onAdd={onAdd}
      rowActions={(b) => (
        <>
          <RowAction onSelect={() => onToggle(b)}>{b.enabled ? "Disable" : "Enable"}</RowAction>
          <RowAction onSelect={() => onRefresh(b.id)}>
            {refreshingId === b.id ? "Updating…" : "Update now"}
          </RowAction>
          <RowAction onSelect={() => onDelete(b.id)} destructive>
            Delete
          </RowAction>
        </>
      )}
    />
  );
}
