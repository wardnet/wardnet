import type { ColumnDef } from "@tanstack/react-table";
import { Badge } from "@/components/core/ui/badge";
import { Button } from "@/components/core/ui/button";
import { DataTable } from "@/components/core/ui/data-table";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import type { CustomFilterRule } from "@wardnet/js";

function createColumns(
  onToggle: (id: string, enabled: boolean) => void,
  onDelete: (id: string) => void,
): ColumnDef<CustomFilterRule>[] {
  return [
    {
      accessorKey: "rule_text",
      header: "Rule",
      cell: ({ row }) => (
        <div className="flex flex-col gap-0.5">
          <span className="font-mono text-sm">{row.original.rule_text}</span>
          {row.original.comment && (
            <span className="text-xs text-muted-foreground">{row.original.comment}</span>
          )}
        </div>
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
        <div className="flex justify-end gap-1">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => onToggle(row.original.id, !row.original.enabled)}
          >
            {row.original.enabled ? "Disable" : "Enable"}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => onDelete(row.original.id)}>
            Delete
          </Button>
        </div>
      ),
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
        actionLabel="Add Rule"
        onAction={onAdd}
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end">
        <Button size="sm" onClick={onAdd}>
          Add Rule
        </Button>
      </div>

      <DataTable columns={columns} data={rules} />
    </div>
  );
}
