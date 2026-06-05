import { useMemo, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Pencil, Trash2 } from "lucide-react";
import { Button } from "@wardnet/forge-web/button";
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Field } from "@wardnet/forge-web/field";
import { Form, Validator } from "@wardnet/forge-web/form";
import { Input } from "@wardnet/forge-web/input";
import { Toggle } from "@wardnet/forge-web/toggle";
import { DataTable, RowAction } from "@/components/core/ui/data-table";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import {
  useDnsZones,
  useDnsRecords,
  useCreateDnsZone,
  useUpdateDnsZone,
  useDeleteDnsZone,
} from "@wardnet/wardnet-web";
import type { DnsZone } from "@wardnet/js";

/** Public-suffix-looking single label or known TLD — making the gateway
 *  authoritative for such a zone would blackhole the real public domain, so
 *  the form warns (without blocking — `.lan` / `home` are perfectly valid). */
const PUBLIC_TLD = /\.(com|net|org|io|dev|app|co|gov|edu)$/i;

/** Authoritative local zones. An enabled zone makes the gateway own the whole
 *  `*.zone` namespace (unknown names answered NXDOMAIN, never forwarded) and
 *  acts as a master switch over its records. */
export function DnsZonesCard() {
  const { data: zoneData } = useDnsZones();
  const { data: recordData } = useDnsRecords();
  const createZone = useCreateDnsZone();
  const updateZone = useUpdateDnsZone();
  const deleteZone = useDeleteDnsZone();

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<DnsZone | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const zones = useMemo(() => zoneData?.zones ?? [], [zoneData]);

  // Per-zone record count, derived from the already-loaded records list.
  const recordCount = useMemo(() => {
    const counts = new Map<string, number>();
    for (const r of recordData?.records ?? []) {
      if (r.zone_id) counts.set(r.zone_id, (counts.get(r.zone_id) ?? 0) + 1);
    }
    return counts;
  }, [recordData]);

  const isSaving = createZone.isPending || updateZone.isPending;
  const zoneToDelete = zones.find((z) => z.id === deleteId);
  const deleteCount = zoneToDelete ? (recordCount.get(zoneToDelete.id) ?? 0) : 0;

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
        cell: ({ row }) => <span className="font-mono text-sm">{row.original.name}</span>,
      },
      {
        id: "records",
        header: "Records",
        meta: { className: "hidden sm:table-cell w-24" },
        cell: ({ row }) => <span className="text-sm">{recordCount.get(row.original.id) ?? 0}</span>,
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
              updateZone.mutate({ id: row.original.id, body: { enabled } })
            }
            disabled={updateZone.isPending}
          />
        ),
      },
    ],
    [recordCount, updateZone],
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
            onCreate={(body) => createZone.mutate(body, { onSuccess: closeForm })}
            onUpdate={(id, body) => updateZone.mutate({ id, body }, { onSuccess: closeForm })}
          />
        )}

        <DataTable
          columns={columns}
          data={zones}
          emptyMessage="No zones yet."
          addLabel="Add zone"
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
        title="Delete zone"
        description={`Delete zone "${zoneToDelete?.name ?? ""}"? Its ${deleteCount} record(s) are kept (they become unzoned), but the gateway stops being authoritative for *.${zoneToDelete?.name ?? ""} — unknown names under it will be forwarded upstream again.`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteId) deleteZone.mutate(deleteId);
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

function ZoneForm({ zone, isSaving, onCancel, onCreate, onUpdate }: ZoneFormProps) {
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
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="home"
              className="w-full sm:w-64"
            />
          </Field>
          <Validator name="name" rule="required" message="Zone name is required." />
          {looksPublic && (
            <p className="text-xs text-warn">
              “{name.trim()}” looks like a public domain. An enabled zone makes the gateway
              authoritative for the whole namespace, which would blackhole the real domain.
            </p>
          )}
        </CardContent>
        <CardFooter className="justify-end gap-2">
          <Button variant="ghost" type="button" onClick={onCancel} disabled={isSaving}>
            Cancel
          </Button>
          <Button type="submit" disabled={isSaving}>
            {zone ? "Save changes" : "Add zone"}
          </Button>
        </CardFooter>
      </Form>
    </Card>
  );
}
