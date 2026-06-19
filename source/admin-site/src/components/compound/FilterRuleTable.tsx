import type { ColumnDef } from "@tanstack/react-table";
import { DataTable, RowAction } from "@/components/core/ui/data-table";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { StatusBadge } from "@/components/compound/StatusBadge";
import { Text } from "@wardnet/web";
import type { CustomFilterRule } from "@wardnet/js";

function createColumns(): ColumnDef<CustomFilterRule>[] {
  return [
    {
      accessorKey: "rule_text",
      header: "Rule",
      // No explicit width — takes remaining space; long rules truncate
      // via the inner spans (see fixedLayout note on DataTable below).
      cell: ({ row }) => (
        <div className="flex min-w-0 flex-col gap-0.5">
          <Text
            as="span"
            size="sm"
            className="truncate font-mono"
            title={row.original.rule_text}
          >
            {row.original.rule_text}
          </Text>
          {row.original.comment && (
            <Text
              as="span"
              size="xs"
              className="truncate text-ink-3"
              title={row.original.comment}
            >
              {row.original.comment}
            </Text>
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
  ];
}

interface FilterRuleTableProps {
  rules: CustomFilterRule[];
  onToggle: (id: string, enabled: boolean) => void;
  onDelete: (id: string) => void;
  onAdd: () => void;
}

/** Table listing custom AdGuard-syntax filter rules. Per-row enable
 *  toggle + delete via the overflow menu; outline-sm "Add rule" in
 *  the toolbar. */
export function FilterRuleTable({
  rules,
  onToggle,
  onDelete,
  onAdd,
}: FilterRuleTableProps) {
  const columns = createColumns();

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
    <DataTable
      columns={columns}
      data={rules}
      fixedLayout
      addLabel="Add rule"
      onAdd={onAdd}
      rowActions={(rule) => (
        <>
          <RowAction onSelect={() => onToggle(rule.id, !rule.enabled)}>
            {rule.enabled ? "Disable" : "Enable"}
          </RowAction>
          <RowAction onSelect={() => onDelete(rule.id)} destructive>
            Delete
          </RowAction>
        </>
      )}
    />
  );
}
