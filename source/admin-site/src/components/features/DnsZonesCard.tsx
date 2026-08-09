import { useMemo, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Pencil, Trash2 } from "lucide-react";
import { FormActions } from "@wardnet/web";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Form, Validator } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { DataTable, RowAction } from "@/components/core/ui/data-table";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import type {
  CreateZoneRequest,
  CustomDnsRecord,
  DnsZone,
  UpdateZoneRequest,
} from "@wardnet/js";

/** Public-suffix-looking single label or known TLD — making the gateway
 *  authoritative for such a zone would blackhole the real public domain, so
 *  the form warns (without blocking — `.lan` / `home` are perfectly valid). */
const PUBLIC_TLD = /\.(com|net|org|io|dev|app|co|gov|edu)$/i;

interface DnsZonesCardProps {
  zones: DnsZone[];
  /** All local records — the card derives per-zone record counts from them. */
  records: CustomDnsRecord[];
  /** True while the page's create or update mutation is in flight. */
  isSaving: boolean;
  /** True while the page's update mutation is in flight (gates the per-row
   *  enable toggles without also locking them during a create). */
  updatePending: boolean;
  /** The optional callbacks match TanStack's `mutate` signature so the page
   *  can pass the mutation's `mutate` straight through; the card uses
   *  `onSuccess` to close its inline form. */
  onCreateZone: (
    body: CreateZoneRequest,
    callbacks?: { onSuccess?: () => void },
  ) => void;
  onUpdateZone: (
    change: { id: string; body: UpdateZoneRequest },
    callbacks?: { onSuccess?: () => void },
  ) => void;
  onDeleteZone: (id: string) => void;
}

/** Authoritative local zones. An enabled zone makes the gateway own the whole
 *  `*.zone` namespace (unknown names answered NXDOMAIN, never forwarded) and
 *  acts as a master switch over its records. Pure presentation — the owning
 *  page wires the query/mutation hooks and passes data + callbacks in. */
export function DnsZonesCard({
  zones,
  records,
  isSaving,
  updatePending,
  onCreateZone,
  onUpdateZone,
  onDeleteZone,
}: DnsZonesCardProps) {
  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<DnsZone | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  // Per-zone record count, derived from the already-loaded records list.
  const recordCount = useMemo(() => {
    const counts = new Map<string, number>();
    for (const r of records) {
      if (r.zone_id) counts.set(r.zone_id, (counts.get(r.zone_id) ?? 0) + 1);
    }
    return counts;
  }, [records]);
  const zoneToDelete = zones.find((z) => z.id === deleteId);
  const deleteCount = zoneToDelete
    ? (recordCount.get(zoneToDelete.id) ?? 0)
    : 0;

  function openCreate() {
    setEditing(null);
    setFormOpen(true);
  }
  function openEdit(zone: DnsZone) {
    setEditing(zone);
    setFormOpen(true);
  }
  function closeForm() {
    setFormOpen(false);
    setEditing(null);
  }

  const columns = useMemo<ColumnDef<DnsZone>[]>(
    () => [
      {
        id: "name",
        header: "Name",
        cell: ({ row }) => (
          <Text as="span" size="sm" className="font-mono">
            {row.original.name}
          </Text>
        ),
      },
      {
        id: "records",
        header: "Records",
        meta: { className: "hidden sm:table-cell w-24" },
        cell: ({ row }) => (
          <Text as="span" size="sm">
            {recordCount.get(row.original.id) ?? 0}
          </Text>
        ),
      },
      {
        id: "enabled",
        header: "Enabled",
        meta: { className: "w-24" },
        cell: ({ row }) => (
          <Toggle
            aria-label={`Toggle zone ${row.original.name}`}
            checked={row.original.enabled}
            onCheckedChange={(enabled) =>
              onUpdateZone({ id: row.original.id, body: { enabled } })
            }
            disabled={updatePending}
          />
        ),
      },
    ],
    [recordCount, onUpdateZone, updatePending],
  );

  return (
    <Card>
      <CardHeader>
        <CardTitle>Zones</CardTitle>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        {formOpen && (
          <ZoneForm
            key={editing?.id ?? "new"}
            zone={editing}
            isSaving={isSaving}
            onCancel={closeForm}
            onCreate={(body) => onCreateZone(body, { onSuccess: closeForm })}
            onUpdate={(id, body) =>
              onUpdateZone({ id, body }, { onSuccess: closeForm })
            }
          />
        )}

        <DataTable
          columns={columns}
          data={zones}
          emptyMessage="No zones yet."
          addLabel="Add zone"
          onAdd={openCreate}
          addTestId="zone-add"
          rowActionsTestId="zone-row-menu"
          rowActions={(row) => (
            <>
              <RowAction
                onSelect={() => openEdit(row)}
                icon={<Pencil aria-hidden />}
                testId="zone-edit"
              >
                Edit
              </RowAction>
              {/* System zones (the daemon-seeded `.lan` zone) can't be deleted —
                  the API rejects it, so don't offer the action. */}
              {row.source !== "system" && (
                <RowAction
                  onSelect={() => setDeleteId(row.id)}
                  destructive
                  icon={<Trash2 aria-hidden />}
                  testId="zone-delete"
                >
                  Delete
                </RowAction>
              )}
            </>
          )}
        />
      </CardContent>

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => {
          if (!open) setDeleteId(null);
        }}
        title="Delete zone"
        description={`Delete zone "${zoneToDelete?.name ?? ""}"? Its ${deleteCount} record(s) are kept (they become unzoned), but the gateway stops being authoritative for *.${zoneToDelete?.name ?? ""} - unknown names under it will be forwarded upstream again.`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteId) onDeleteZone(deleteId);
          setDeleteId(null);
        }}
      />
    </Card>
  );
}

interface ZoneFormProps {
  zone: DnsZone | null;
  isSaving: boolean;
  onCancel: () => void;
  onCreate: (body: { name: string; enabled: boolean }) => void;
  onUpdate: (id: string, body: { name: string }) => void;
}

function ZoneForm({
  zone,
  isSaving,
  onCancel,
  onCreate,
  onUpdate,
}: ZoneFormProps) {
  const [name, setName] = useState(zone?.name ?? "");
  const looksPublic = PUBLIC_TLD.test(name.trim());

  function handleSave(values: { name: string }) {
    const trimmed = values.name.trim();
    if (zone) {
      onUpdate(zone.id, { name: trimmed });
    } else {
      onCreate({ name: trimmed, enabled: true });
    }
  }

  return (
    <Card className="border-dashed">
      <CardHeader>
        <CardTitle>{zone ? "Edit zone" : "Add zone"}</CardTitle>
      </CardHeader>
      <Form values={{ name }} onSubmit={handleSave}>
        <CardContent className="flex flex-col gap-3">
          <Field
            label="Zone name"
            htmlFor="zone-name"
            name="name"
            help="A local domain the gateway answers for, e.g. lan or home. Single-label names are valid."
          >
            <Input
              id="zone-name"
              data-testid="zone-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="home"
              className="w-full sm:w-64"
            />
          </Field>
          <Validator
            name="name"
            rule="required"
            message="Zone name is required."
          />
          {looksPublic && (
            <Text as="p" size="xs" className="text-warn">
              “{name.trim()}” looks like a public domain. An enabled zone makes
              the gateway authoritative for the whole namespace, which would
              blackhole the real domain.
            </Text>
          )}
        </CardContent>
        <FormActions
          secondaryLabel="Cancel"
          secondaryProps={{
            type: "button",
            onClick: onCancel,
            disabled: isSaving,
          }}
          primaryLabel={zone ? "Save changes" : "Add zone"}
          primaryProps={{
            type: "submit",
            disabled: isSaving,
            "data-testid": "zone-submit",
          }}
        />
      </Form>
    </Card>
  );
}
