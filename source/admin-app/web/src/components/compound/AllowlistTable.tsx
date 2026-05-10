import type { ColumnDef } from "@tanstack/react-table";
import { MoreHorizontalIcon } from "lucide-react";
import { Button } from "@wardnet/forge-web/button";
import { DataTable } from "@/components/core/ui/data-table";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@wardnet/forge-web/dropdown-menu";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { timeAgo } from "@/lib/utils";
import type { AllowlistEntry } from "@wardnet/js";

function createColumns(onDelete: (id: string) => void): ColumnDef<AllowlistEntry>[] {
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
          {row.original.reason ?? "\u2014"}
        </span>
      ),
    },
    {
      accessorKey: "created_at",
      header: "Added",
      meta: { className: "hidden w-32 md:table-cell" },
      cell: ({ row }) => <span className="text-ink-3">{timeAgo(row.original.created_at)}</span>,
    },
    {
      id: "actions",
      header: "",
      meta: { className: "w-12 text-right" },
      cell: ({ row }) => (
        <div className="flex justify-end" onClick={(e) => e.stopPropagation()}>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={`Actions for ${row.original.domain}`}
              >
                <MoreHorizontalIcon />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent>
              <DropdownMenuItem variant="destructive" onSelect={() => onDelete(row.original.id)}>
                Remove
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
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
        actionLabel="Add domain"
        onAction={onAdd}
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end">
        <Button variant="outline" onClick={onAdd}>
          Add domain
        </Button>
      </div>

      <DataTable columns={columns} data={entries} fixedLayout />
    </div>
  );
}
