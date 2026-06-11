import { useMemo, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Pencil, Trash2 } from "lucide-react";
import { Button } from "@wardnet/web";
import {
  Card,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Form, Validator } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Pill } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import { DataTable, RowAction } from "@/components/core/ui/data-table";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import {
  useDnsRecords,
  useDnsZones,
  useCreateDnsRecord,
  useUpdateDnsRecord,
  useDeleteDnsRecord,
} from "@wardnet/web";
import type { CustomDnsRecord, DnsRecordType } from "@wardnet/js";

const RECORD_TYPES: DnsRecordType[] = [
  "A",
  "AAAA",
  "CNAME",
  "TXT",
  "MX",
  "SRV",
];
/** Sentinel for the "no zone" Select option — Radix Select can't hold an
 *  empty string value, so we map this to `zone_id: null` on submit. */
const NO_ZONE = "__none__";

/** Custom local DNS records — the manual CRUD surface. DHCP/system records
 *  are intentionally excluded here (they're surfaced in the DHCP `.lan`
 *  info card); only `source === "manual"` records are editable. */
export function LocalRecordsCard() {
  const { data: recordData } = useDnsRecords();
  const { data: zoneData } = useDnsZones();
  const createRecord = useCreateDnsRecord();
  const updateRecord = useUpdateDnsRecord();
  const deleteRecord = useDeleteDnsRecord();

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<CustomDnsRecord | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const zones = useMemo(() => zoneData?.zones ?? [], [zoneData]);
  const zoneName = useMemo(() => {
    const map = new Map(zones.map((z) => [z.id, z.name]));
    return (id?: string | null) => (id ? (map.get(id) ?? "—") : "—");
  }, [zones]);

  const records = useMemo(
    () => (recordData?.records ?? []).filter((r) => r.source === "manual"),
    [recordData],
  );

  const isSaving = createRecord.isPending || updateRecord.isPending;
  const recordToDelete = records.find((r) => r.id === deleteId);

  function openCreate() {
    setEditing(null);
    setFormOpen(true);
  }
  function openEdit(record: CustomDnsRecord) {
    setEditing(record);
    setFormOpen(true);
  }
  function closeForm() {
    setFormOpen(false);
    setEditing(null);
  }

  const columns = useMemo<ColumnDef<CustomDnsRecord>[]>(
    () => [
      {
        id: "domain",
        header: "Domain",
        cell: ({ row }) => (
          <span className="font-mono text-xs">{row.original.domain}</span>
        ),
      },
      {
        id: "type",
        header: "Type",
        meta: { className: "w-20" },
        cell: ({ row }) => (
          <Pill variant="ghost">{row.original.record_type}</Pill>
        ),
      },
      {
        id: "value",
        header: "Value",
        cell: ({ row }) => (
          <span className="font-mono text-xs">{row.original.value}</span>
        ),
      },
      {
        id: "ttl",
        header: "TTL",
        meta: { className: "hidden md:table-cell w-20" },
        cell: ({ row }) => <span className="text-sm">{row.original.ttl}</span>,
      },
      {
        id: "zone",
        header: "Zone",
        meta: { className: "hidden sm:table-cell" },
        cell: ({ row }) => (
          <span className="text-sm">{zoneName(row.original.zone_id)}</span>
        ),
      },
      {
        id: "enabled",
        header: "Enabled",
        meta: { className: "w-24" },
        cell: ({ row }) => (
          <Toggle
            aria-label={`Toggle ${row.original.domain}`}
            checked={row.original.enabled}
            onCheckedChange={(enabled) =>
              updateRecord.mutate({ id: row.original.id, body: { enabled } })
            }
            disabled={updateRecord.isPending}
          />
        ),
      },
    ],
    [zoneName, updateRecord],
  );

  return (
    <Card>
      <CardHeader>
        <CardTitle>Records</CardTitle>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        {formOpen && (
          <RecordForm
            // Remount with fresh state whenever the edit target changes.
            key={editing?.id ?? "new"}
            record={editing}
            zones={zones}
            isSaving={isSaving}
            onCancel={closeForm}
            onCreate={(body) =>
              createRecord.mutate(body, { onSuccess: closeForm })
            }
            onUpdate={(id, body) =>
              updateRecord.mutate({ id, body }, { onSuccess: closeForm })
            }
          />
        )}

        <DataTable
          columns={columns}
          data={records}
          emptyMessage="No custom records yet."
          addLabel="Add record"
          onAdd={openCreate}
          rowActions={(row) => (
            <>
              <RowAction
                onSelect={() => openEdit(row)}
                icon={<Pencil aria-hidden />}
              >
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
        title="Delete record"
        description={`Delete ${recordToDelete?.record_type ?? ""} record for ${recordToDelete?.domain ?? "this name"}? Clients will stop resolving it locally.`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteId) deleteRecord.mutate(deleteId);
          setDeleteId(null);
        }}
      />
    </Card>
  );
}

interface RecordFormProps {
  record: CustomDnsRecord | null;
  zones: { id: string; name: string }[];
  isSaving: boolean;
  onCancel: () => void;
  onCreate: (body: {
    zone_id: string | null;
    domain: string;
    record_type: DnsRecordType;
    value: string;
    ttl: number;
    enabled: boolean;
  }) => void;
  onUpdate: (
    id: string,
    body: {
      zone_id: string | null;
      domain: string;
      record_type: DnsRecordType;
      value: string;
      ttl: number;
    },
  ) => void;
}

function RecordForm({
  record,
  zones,
  isSaving,
  onCancel,
  onCreate,
  onUpdate,
}: RecordFormProps) {
  const [domain, setDomain] = useState(record?.domain ?? "");
  const [recordType, setRecordType] = useState<DnsRecordType>(
    record?.record_type ?? "A",
  );
  const [value, setValue] = useState(record?.value ?? "");
  const [ttl, setTtl] = useState(String(record?.ttl ?? 300));
  const [zoneId, setZoneId] = useState(record?.zone_id ?? NO_ZONE);

  function handleSave(values: { domain: string; value: string; ttl: string }) {
    const resolvedZone = zoneId === NO_ZONE ? null : zoneId;
    const ttlNum = Number(values.ttl);
    const shared = {
      zone_id: resolvedZone,
      domain: values.domain.trim(),
      record_type: recordType,
      value: values.value.trim(),
      ttl: Number.isFinite(ttlNum) && ttlNum >= 0 ? ttlNum : 300,
    };
    if (record) {
      onUpdate(record.id, shared);
    } else {
      onCreate({ ...shared, enabled: true });
    }
  }

  return (
    <Card className="border-dashed">
      <CardHeader>
        <CardTitle>{record ? "Edit record" : "Add record"}</CardTitle>
      </CardHeader>
      <Form values={{ domain, value, ttl }} onSubmit={handleSave}>
        <CardContent className="flex flex-col gap-5">
          <div className="flex gap-3">
            <Field
              label="Domain"
              htmlFor="rec-domain"
              name="domain"
              className="flex-1"
            >
              <Input
                id="rec-domain"
                value={domain}
                onChange={(e) => setDomain(e.target.value)}
                placeholder="printer.lan"
              />
            </Field>
            <Validator
              name="domain"
              rule="required"
              message="Domain is required."
            />

            <Field label="Type" htmlFor="rec-type" className="w-28">
              <Select
                value={recordType}
                onValueChange={(v) => setRecordType(v as DnsRecordType)}
              >
                <SelectTrigger id="rec-type">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {RECORD_TYPES.map((t) => (
                    <SelectItem key={t} value={t}>
                      {t}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
          </div>

          <div className="flex gap-3">
            <Field
              label="Value"
              htmlFor="rec-value"
              name="value"
              className="flex-1"
            >
              <Input
                id="rec-value"
                value={value}
                onChange={(e) => setValue(e.target.value)}
                placeholder="192.168.1.50"
              />
            </Field>
            <Validator
              name="value"
              rule="required"
              message="Value is required."
            />

            <Field
              label="TTL (s)"
              htmlFor="rec-ttl"
              name="ttl"
              className="w-28"
            >
              <Input
                id="rec-ttl"
                type="number"
                min={0}
                value={ttl}
                onChange={(e) => setTtl(e.target.value)}
              />
            </Field>
            <Validator name="ttl" rule="required" message="TTL is required." />
          </div>

          <Field
            label="Zone"
            htmlFor="rec-zone"
            help="Optional. Unzoned records always resolve; a zone lets you toggle a whole group at once."
          >
            <Select value={zoneId} onValueChange={setZoneId}>
              <SelectTrigger id="rec-zone" className="w-full sm:w-64">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={NO_ZONE}>No zone (unzoned)</SelectItem>
                {zones.map((z) => (
                  <SelectItem key={z.id} value={z.id}>
                    {z.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
        </CardContent>
        <CardFooter className="justify-end gap-2">
          <Button
            variant="ghost"
            type="button"
            onClick={onCancel}
            disabled={isSaving}
          >
            Cancel
          </Button>
          <Button type="submit" disabled={isSaving}>
            {record ? "Save changes" : "Add record"}
          </Button>
        </CardFooter>
      </Form>
    </Card>
  );
}
