import { useMemo, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Pencil, Trash2 } from "lucide-react";
import { FormActions } from "@wardnet/web";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Form, Validator } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Pill } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import { DataTable, RowAction } from "@/components/core/ui/data-table";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import type {
  CreateRecordRequest,
  CustomDnsRecord,
  DnsRecordType,
  DnsZone,
  UpdateRecordRequest,
} from "@wardnet/js";

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

interface LocalRecordsCardProps {
  /** All local records — the card shows only the editable `manual` ones. */
  records: CustomDnsRecord[];
  zones: DnsZone[];
  /** True while the page's create or update mutation is in flight. */
  isSaving: boolean;
  /** True while the page's update mutation is in flight (gates the per-row
   *  enable toggles without also locking them during a create). */
  updatePending: boolean;
  /** The optional callbacks match TanStack's `mutate` signature so the page
   *  can pass the mutation's `mutate` straight through; the card uses
   *  `onSuccess` to close its inline form. */
  onCreateRecord: (
    body: CreateRecordRequest,
    callbacks?: { onSuccess?: () => void },
  ) => void;
  onUpdateRecord: (
    change: { id: string; body: UpdateRecordRequest },
    callbacks?: { onSuccess?: () => void },
  ) => void;
  onDeleteRecord: (id: string) => void;
}

/** Custom local DNS records — the manual CRUD surface. DHCP/system records
 *  are intentionally excluded here (they're surfaced in the DHCP `.lan`
 *  info card); only `source === "manual"` records are editable. Pure
 *  presentation — the owning page wires the query/mutation hooks and passes
 *  data + callbacks in. */
export function LocalRecordsCard({
  records: allRecords,
  zones,
  isSaving,
  updatePending,
  onCreateRecord,
  onUpdateRecord,
  onDeleteRecord,
}: LocalRecordsCardProps) {
  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<CustomDnsRecord | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const zoneName = useMemo(() => {
    const map = new Map(zones.map((z) => [z.id, z.name]));
    return (id?: string | null) => (id ? (map.get(id) ?? "-") : "-");
  }, [zones]);

  const records = useMemo(
    () => allRecords.filter((r) => r.source === "manual"),
    [allRecords],
  );

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
          <Text as="span" size="xs" className="font-mono">
            {row.original.domain}
          </Text>
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
          <Text as="span" size="xs" className="font-mono">
            {row.original.value}
          </Text>
        ),
      },
      {
        id: "ttl",
        header: "TTL",
        meta: { className: "hidden md:table-cell w-20" },
        cell: ({ row }) => (
          <Text as="span" size="sm">
            {row.original.ttl}
          </Text>
        ),
      },
      {
        id: "zone",
        header: "Zone",
        meta: { className: "hidden sm:table-cell" },
        cell: ({ row }) => (
          <Text as="span" size="sm">
            {zoneName(row.original.zone_id)}
          </Text>
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
              onUpdateRecord({ id: row.original.id, body: { enabled } })
            }
            disabled={updatePending}
          />
        ),
      },
    ],
    [zoneName, onUpdateRecord, updatePending],
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
            onCreate={(body) => onCreateRecord(body, { onSuccess: closeForm })}
            onUpdate={(id, body) =>
              onUpdateRecord({ id, body }, { onSuccess: closeForm })
            }
          />
        )}

        <DataTable
          columns={columns}
          data={records}
          emptyMessage="No custom records yet."
          addLabel="Add record"
          onAdd={openCreate}
          addTestId="local-record-add"
          rowActionsTestId="local-record-row-menu"
          rowActions={(row) => (
            <>
              <RowAction
                onSelect={() => openEdit(row)}
                icon={<Pencil aria-hidden />}
                testId="local-record-edit"
              >
                Edit
              </RowAction>
              <RowAction
                onSelect={() => setDeleteId(row.id)}
                destructive
                icon={<Trash2 aria-hidden />}
                testId="local-record-delete"
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
          if (deleteId) onDeleteRecord(deleteId);
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
                data-testid="local-record-domain"
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
                data-testid="local-record-value"
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
                data-testid="local-record-ttl"
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
        <FormActions
          secondaryLabel="Cancel"
          secondaryProps={{
            type: "button",
            onClick: onCancel,
            disabled: isSaving,
          }}
          primaryLabel={record ? "Save changes" : "Add record"}
          primaryProps={{
            type: "submit",
            disabled: isSaving,
            "data-testid": "local-record-submit",
          }}
        />
      </Form>
    </Card>
  );
}
