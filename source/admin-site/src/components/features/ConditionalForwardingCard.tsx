import { useMemo, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Pencil, Trash2 } from "lucide-react";
import { Button } from "@wardnet/forge-web/button";
import {
  Card,
  CardContent,
  CardFooter,
  CardHeader,
  CardSubtitle,
  CardTitle,
} from "@wardnet/forge-web/card";
import { Field } from "@wardnet/forge-web/field";
import { Form, Validator } from "@wardnet/forge-web/form";
import { Input } from "@wardnet/forge-web/input";
import { Toggle } from "@wardnet/forge-web/toggle";
import { DataTable, RowAction } from "@/components/core/ui/data-table";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import {
  useForwardingRules,
  useCreateForwardingRule,
  useUpdateForwardingRule,
  useDeleteForwardingRule,
} from "@wardnet/wardnet-web";
import type { ConditionalForwardingRule } from "@wardnet/js";

/** Conditional forwarding — send a specific domain to a chosen upstream
 *  resolver instead of the default. An explicit rule overrides authoritative
 *  zone handling for the same name. */
export function ConditionalForwardingCard() {
  const { data } = useForwardingRules();
  const createRule = useCreateForwardingRule();
  const updateRule = useUpdateForwardingRule();
  const deleteRule = useDeleteForwardingRule();

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<ConditionalForwardingRule | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const rules = useMemo(() => data?.rules ?? [], [data]);
  const isSaving = createRule.isPending || updateRule.isPending;
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
        cell: ({ row }) => <span className="font-mono text-xs">{row.original.domain}</span>,
      },
      {
        id: "upstream",
        header: "Upstream",
        cell: ({ row }) => <span className="font-mono text-xs">{row.original.upstream}</span>,
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
              updateRule.mutate({ id: row.original.id, body: { enabled } })
            }
            disabled={updateRule.isPending}
          />
        ),
      },
    ],
    [updateRule],
  );

  return (
    <Card>
      <CardHeader>
        <CardTitle>Conditional forwarding</CardTitle>
        <CardSubtitle>
          Send queries for a specific domain to a chosen DNS server, instead of the default
          resolver.
        </CardSubtitle>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        {formOpen && (
          <RuleForm
            key={editing?.id ?? "new"}
            rule={editing}
            isSaving={isSaving}
            onCancel={closeForm}
            onCreate={(body) => createRule.mutate(body, { onSuccess: closeForm })}
            onUpdate={(id, body) => updateRule.mutate({ id, body }, { onSuccess: closeForm })}
          />
        )}

        <DataTable
          columns={columns}
          data={rules}
          emptyMessage="No forwarding rules yet."
          addLabel="Add rule"
          onAdd={openCreate}
          rowActions={(row) => (
            <>
              <RowAction onSelect={() => openEdit(row)} icon={<Pencil aria-hidden />}>
                Edit
              </RowAction>
              <RowAction
                onSelect={() => setDeleteId(row.id)}
                destructive
                icon={<Trash2 aria-hidden />}
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
          if (deleteId) deleteRule.mutate(deleteId);
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
  onCreate: (body: { domain: string; upstream: string; enabled: boolean }) => void;
  onUpdate: (id: string, body: { domain: string; upstream: string }) => void;
}

function RuleForm({ rule, isSaving, onCancel, onCreate, onUpdate }: RuleFormProps) {
  const [domain, setDomain] = useState(rule?.domain ?? "");
  const [upstream, setUpstream] = useState(rule?.upstream ?? "");

  function handleSave(values: { domain: string; upstream: string }) {
    const shared = { domain: values.domain.trim(), upstream: values.upstream.trim() };
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
            <Field label="Domain" htmlFor="fwd-domain" name="domain" className="flex-1">
              <Input
                id="fwd-domain"
                value={domain}
                onChange={(e) => setDomain(e.target.value)}
                placeholder="corp.internal"
              />
            </Field>
            <Validator name="domain" rule="required" message="Domain is required." />

            <Field label="Upstream" htmlFor="fwd-upstream" name="upstream" className="flex-1">
              <Input
                id="fwd-upstream"
                value={upstream}
                onChange={(e) => setUpstream(e.target.value)}
                placeholder="10.0.0.1"
              />
            </Field>
            <Validator name="upstream" rule="required" message="Upstream is required." />
          </div>
        </CardContent>
        <CardFooter className="justify-end gap-2">
          <Button variant="ghost" type="button" onClick={onCancel} disabled={isSaving}>
            Cancel
          </Button>
          <Button type="submit" disabled={isSaving}>
            {rule ? "Save changes" : "Add rule"}
          </Button>
        </CardFooter>
      </Form>
    </Card>
  );
}
