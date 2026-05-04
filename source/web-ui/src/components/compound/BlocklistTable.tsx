import type { ColumnDef } from "@tanstack/react-table";
import { MoreHorizontalIcon } from "lucide-react";
import { Button } from "@/components/core/ui/button";
import { DataTable } from "@/components/core/ui/data-table";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/core/ui/dropdown-menu";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { StatusBadge } from "@/components/compound/StatusBadge";
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
      header: "Last updated",
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
        <StatusBadge tone={row.original.enabled ? "success" : "neutral"}>
          {row.original.enabled ? "Enabled" : "Disabled"}
        </StatusBadge>
      ),
    },
    {
      id: "actions",
      header: "",
      meta: { className: "text-right" },
      cell: ({ row }) => {
        const blocklist = row.original;
        const isRefreshing = refreshingId === blocklist.id;
        return (
          <div className="flex justify-end" onClick={(e) => e.stopPropagation()}>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon-sm" aria-label={`Actions for ${blocklist.name}`}>
                  <MoreHorizontalIcon />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                <DropdownMenuItem onSelect={() => onToggle(blocklist)}>
                  {blocklist.enabled ? "Disable" : "Enable"}
                </DropdownMenuItem>
                <DropdownMenuItem disabled={isRefreshing} onSelect={() => onRefresh(blocklist.id)}>
                  {isRefreshing ? "Updating…" : "Update now"}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem variant="destructive" onSelect={() => onDelete(blocklist.id)}>
                  Delete
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        );
      },
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
        actionLabel="Add blocklist"
        onAction={onAdd}
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end">
        <Button onClick={onAdd}>Add blocklist</Button>
      </div>

      <DataTable columns={columns} data={blocklists} onRowClick={onEdit} />
    </div>
  );
}
