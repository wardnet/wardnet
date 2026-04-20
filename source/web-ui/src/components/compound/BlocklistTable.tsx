import type { ColumnDef } from "@tanstack/react-table";
import { Badge } from "@/components/core/ui/badge";
import { Button } from "@/components/core/ui/button";
import { DataTable } from "@/components/core/ui/data-table";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { timeAgo } from "@/lib/utils";
import type { Blocklist } from "@wardnet/js";

function createColumns(
  onRefresh: (id: string) => void,
  onToggle: (blocklist: Blocklist) => void,
  onDelete: (id: string) => void,
  refreshingId: string | null,
): ColumnDef<Blocklist>[] {
  return [
    {
      accessorKey: "name",
      header: "Name",
      cell: ({ row }) => (
        <div className="flex flex-col gap-0.5">
          <span className="font-medium">{row.original.name}</span>
          <span className="font-mono text-xs text-muted-foreground">{row.original.url}</span>
          {row.original.last_error && (
            <span className="text-xs text-destructive">{row.original.last_error}</span>
          )}
        </div>
      ),
    },
    {
      accessorKey: "entry_count",
      header: "Entries",
      meta: { className: "hidden sm:table-cell" },
      cell: ({ row }) => (
        <span className="tabular-nums">{row.original.entry_count.toLocaleString()}</span>
      ),
    },
    {
      accessorKey: "last_updated",
      header: "Last Updated",
      meta: { className: "hidden md:table-cell" },
      cell: ({ row }) => (
        <span className="text-muted-foreground">
          {row.original.last_updated ? timeAgo(row.original.last_updated) : "Never"}
        </span>
      ),
    },
    {
      accessorKey: "enabled",
      header: "Status",
      cell: ({ row }) => (
        <Badge variant={row.original.enabled ? "default" : "secondary"}>
          {row.original.enabled ? "Enabled" : "Disabled"}
        </Badge>
      ),
    },
    {
      id: "actions",
      header: "",
      meta: { className: "text-right" },
      cell: ({ row }) => (
        <div className="flex justify-end gap-1" onClick={(e) => e.stopPropagation()}>
          <Button variant="ghost" size="sm" onClick={() => onToggle(row.original)}>
            {row.original.enabled ? "Disable" : "Enable"}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => onRefresh(row.original.id)}
            disabled={refreshingId === row.original.id}
          >
            {refreshingId === row.original.id ? "Updating..." : "Update Now"}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => onDelete(row.original.id)}>
            Delete
          </Button>
        </div>
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

/** Table listing blocklists. Row click opens the edit sheet; actions column
 *  provides inline toggle, refresh, and delete. */
export function BlocklistTable({
  blocklists,
  onRefresh,
  onToggle,
  onEdit,
  onDelete,
  refreshingId = null,
  onAdd,
}: BlocklistTableProps) {
  const columns = createColumns(onRefresh, onToggle, onDelete, refreshingId);

  if (blocklists.length === 0) {
    return (
      <EmptyStatePlaceholder
        message="No blocklists configured"
        hint="Add a blocklist URL to start blocking ads and trackers network-wide."
        actionLabel="Add Blocklist"
        onAction={onAdd}
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end">
        <Button size="sm" onClick={onAdd}>
          Add Blocklist
        </Button>
      </div>

      <DataTable columns={columns} data={blocklists} onRowClick={onEdit} />
    </div>
  );
}
