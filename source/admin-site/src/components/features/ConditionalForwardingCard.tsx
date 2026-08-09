import { useMemo, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Pencil, Trash2 } from "lucide-react";
import { FormActions } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  Card,
  CardContent,
  CardHeader,
  CardSubtitle,
  CardTitle,
} from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Form, Validator } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import { DataTable, RowAction } from "@/components/core/ui/data-table";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import type { MutateFn } from "@/lib/mutationHandle";
import type {
  ConditionalForwardingRule,
  CreateForwardingRuleRequest,
  UpdateForwardingRuleRequest,
} from "@wardnet/js";

interface ConditionalForwardingCardProps {
  rules: ConditionalForwardingRule[];
  /** True while the page's create or update mutation is in flight. */
  isSaving: boolean;
  /** True while the page's update mutation is in flight (gates the per-row
   *  enable toggles without also locking them during a create). */
  updatePending: boolean;
  onCreateRule: MutateFn<CreateForwardingRuleRequest>;
  onUpdateRule: MutateFn<{ id: string; body: UpdateForwardingRuleRequest }>;
  onDeleteRule: (id: string) => void;
}

/** Conditional forwarding — send a specific domain to a chosen upstream
 *  resolver instead of the default. An explicit rule overrides authoritative
 *  zone handling for the same name. Pure presentation — the owning page wires
 *  the query/mutation hooks and passes data + callbacks in. */
export function ConditionalForwardingCard({
  rules,
  isSaving,
  updatePending,
  onCreateRule,
  onUpdateRule,
  onDeleteRule,
}: ConditionalForwardingCardProps) {
  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<ConditionalForwardingRule | null>(
    null,
  );
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const ruleToDelete = rules.find((r) => r.id === deleteId);

  function openCreate() {
    setEditing(null);
    setFormOpen(true);
  }
  function openEdit(rule: ConditionalForwardingRule) {
    setEditing(rule);
    setFormOpen(true);
  }
  function closeForm() {
    setFormOpen(false);
    setEditing(null);
  }

  const columns = useMemo<ColumnDef<ConditionalForwardingRule>[]>(
    () => [
      {
        id: "domain",
        header: "Domain",
        cell: ({ row }) => (
          <Text size="xs" className="font-mono">
            {row.original.domain}
          </Text>
        ),
      },
      {
        id: "upstream",
        header: "Upstream",
        cell: ({ row }) => (
          <Text size="xs" className="font-mono">
            {row.original.upstream}
          </Text>
        ),
      },
      {
        id: "enabled",
        header: "Enabled",
        meta: { className: "w-24" },
        cell: ({ row }) => (
          <Toggle
            aria-label={`Toggle rule for ${row.original.domain}`}
            checked={row.original.enabled}
            onCheckedChange={(enabled) =>
              onUpdateRule({ id: row.original.id, body: { enabled } })
            }
            disabled={updatePending}
          />
        ),
      },
    ],
    [onUpdateRule, updatePending],
  );

  return (
    <Card>
      <CardHeader>
        <CardTitle>Conditional forwarding</CardTitle>
        <CardSubtitle>
          Send queries for a specific domain to a chosen DNS server, instead of
          the default resolver.
        </CardSubtitle>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        {formOpen && (
          <RuleForm
            key={editing?.id ?? "new"}
            rule={editing}
            isSaving={isSaving}
            onCancel={closeForm}
            onCreate={(body) => onCreateRule(body, { onSuccess: closeForm })}
            onUpdate={(id, body) =>
              onUpdateRule({ id, body }, { onSuccess: closeForm })
            }
          />
        )}

        <DataTable
          columns={columns}
          data={rules}
          emptyMessage="No forwarding rules yet."
          addLabel="Add rule"
          onAdd={openCreate}
          addTestId="fwd-add"
          rowActionsTestId="fwd-row-menu"
          rowActions={(row) => (
            <>
              <RowAction
                onSelect={() => openEdit(row)}
                icon={<Pencil aria-hidden />}
                testId="fwd-edit"
              >
                Edit
              </RowAction>
              <RowAction
                onSelect={() => setDeleteId(row.id)}
                destructive
                icon={<Trash2 aria-hidden />}
                testId="fwd-delete"
              >
                Delete
              </RowAction>
            </>
          )}
        />
      </CardContent>

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => {
          if (!open) setDeleteId(null);
        }}
        title="Delete forwarding rule"
        description={`Delete the forwarding rule for ${ruleToDelete?.domain ?? "this domain"}? Queries will fall back to the default upstream.`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteId) onDeleteRule(deleteId);
          setDeleteId(null);
        }}
      />
    </Card>
  );
}

interface RuleFormProps {
  rule: ConditionalForwardingRule | null;
  isSaving: boolean;
  onCancel: () => void;
  onCreate: (body: {
    domain: string;
    upstream: string;
    enabled: boolean;
  }) => void;
  onUpdate: (id: string, body: { domain: string; upstream: string }) => void;
}

function RuleForm({
  rule,
  isSaving,
  onCancel,
  onCreate,
  onUpdate,
}: RuleFormProps) {
  const [domain, setDomain] = useState(rule?.domain ?? "");
  const [upstream, setUpstream] = useState(rule?.upstream ?? "");

  function handleSave(values: { domain: string; upstream: string }) {
    const shared = {
      domain: values.domain.trim(),
      upstream: values.upstream.trim(),
    };
    if (rule) {
      onUpdate(rule.id, shared);
    } else {
      onCreate({ ...shared, enabled: true });
    }
  }

  return (
    <Card className="border-dashed">
      <CardHeader>
        <CardTitle>{rule ? "Edit rule" : "Add rule"}</CardTitle>
      </CardHeader>
      <Form values={{ domain, upstream }} onSubmit={handleSave}>
        <CardContent className="flex flex-col gap-5">
          <div className="flex gap-3">
            <Field
              label="Domain"
              htmlFor="fwd-domain"
              name="domain"
              className="flex-1"
            >
              <Input
                id="fwd-domain"
                data-testid="fwd-domain"
                value={domain}
                onChange={(e) => setDomain(e.target.value)}
                placeholder="corp.internal"
              />
            </Field>
            <Validator
              name="domain"
              rule="required"
              message="Domain is required."
            />

            <Field
              label="Upstream"
              htmlFor="fwd-upstream"
              name="upstream"
              className="flex-1"
            >
              <Input
                id="fwd-upstream"
                data-testid="fwd-upstream"
                value={upstream}
                onChange={(e) => setUpstream(e.target.value)}
                placeholder="10.0.0.1"
              />
            </Field>
            <Validator
              name="upstream"
              rule="required"
              message="Upstream is required."
            />
          </div>
        </CardContent>
        <FormActions
          secondaryLabel="Cancel"
          secondaryProps={{
            type: "button",
            onClick: onCancel,
            disabled: isSaving,
          }}
          primaryLabel={rule ? "Save changes" : "Add rule"}
          primaryProps={{
            type: "submit",
            disabled: isSaving,
            "data-testid": "fwd-submit",
          }}
        />
      </Form>
    </Card>
  );
}
