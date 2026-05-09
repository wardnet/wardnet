import type { ColumnDef } from "@tanstack/react-table";
import { MoreHorizontalIcon } from "lucide-react";
import { Button } from "@wardnet/forge/button";
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
import type { CustomFilterRule } from "@wardnet/js";

function createColumns(
  onToggle: (id: string, enabled: boolean) => void,
  onDelete: (id: string) => void,
): ColumnDef<CustomFilterRule>[] {
  return [
    {
      accessorKey: "rule_text",
      header: "Rule",
      // No explicit width — takes remaining space; long rules truncate
      // via the inner spans (see fixedLayout note on DataTable below).
      cell: ({ row }) => (
        <div className="flex min-w-0 flex-col gap-0.5">
          <span className="truncate font-mono text-sm" title={row.original.rule_text}>
            {row.original.rule_text}
          </span>
          {row.original.comment && (
            <span className="truncate text-xs text-muted-foreground" title={row.original.comment}>
              {row.original.comment}
            </span>
          )}
        </div>
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
    {
      id: "actions",
      header: "",
      meta: { className: "w-12 text-right" },
      cell: ({ row }) => {
        const rule = row.original;
        return (
          <div className="flex justify-end" onClick={(e) => e.stopPropagation()}>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon-sm" aria-label={`Actions for ${rule.rule_text}`}>
                  <MoreHorizontalIcon />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                <DropdownMenuItem onSelect={() => onToggle(rule.id, !rule.enabled)}>
                  {rule.enabled ? "Disable" : "Enable"}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem variant="destructive" onSelect={() => onDelete(rule.id)}>
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

interface FilterRuleTableProps {
  rules: CustomFilterRule[];
  onToggle: (id: string, enabled: boolean) => void;
  onDelete: (id: string) => void;
  onAdd: () => void;
}

/** Table listing custom AdGuard-syntax filter rules. */
export function FilterRuleTable({ rules, onToggle, onDelete, onAdd }: FilterRuleTableProps) {
  const columns = createColumns(onToggle, onDelete);

  if (rules.length === 0) {
    return (
      <EmptyStatePlaceholder
        message="No custom filter rules"
        hint="Add AdGuard-syntax rules for fine-grained control over what gets blocked or allowed."
        actionLabel="Add rule"
        onAction={onAdd}
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end">
        <Button variant="outline" onClick={onAdd}>
          Add rule
        </Button>
      </div>

      <DataTable columns={columns} data={rules} fixedLayout />
    </div>
  );
}
