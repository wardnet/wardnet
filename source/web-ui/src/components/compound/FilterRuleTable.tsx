import type { ColumnDef } from "@tanstack/react-table";
import { Button } from "@/components/core/ui/button";
import { DataTable } from "@/components/core/ui/data-table";
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
        <StatusBadge tone={row.original.enabled ? "success" : "neutral"}>
          {row.original.enabled ? "Enabled" : "Disabled"}
        </StatusBadge>
      ),
    },
    {
      id: "actions",
      header: "",
      meta: { className: "text-right" },
      cell: ({ row }) => (
        <div className="flex justify-end gap-4">
          <Button
            variant="tertiary"
            onClick={() => onToggle(row.original.id, !row.original.enabled)}
          >
            {row.original.enabled ? "Disable" : "Enable"}
          </Button>
          <Button
            variant="tertiary"
            className="text-destructive hover:text-destructive"
            onClick={() => onDelete(row.original.id)}
          >
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
        actionLabel="Add rule"
        onAction={onAdd}
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-end">
        <Button onClick={onAdd}>Add rule</Button>
      </div>

      <DataTable columns={columns} data={rules} />
    </div>
  );
}
