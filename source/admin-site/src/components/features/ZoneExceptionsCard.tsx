import { useMemo, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Trash2 } from "lucide-react";
import { Plus as PlusIcon } from "lucide-react";
import { Button } from "@wardnet/web";
import { FormActions } from "@wardnet/web";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Pill } from "@wardnet/web";
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
import {
  useDevices,
  useNetworkZones,
  useZoneExceptions,
  useCreateZoneException,
  useDeleteZoneException,
  deviceDisplayName,
} from "@wardnet/web";
import type {
  ExceptionEndpoint,
  ExceptionEndpointKind,
  PortSpec,
  Proto,
  ServiceSpec,
  ZoneException,
} from "@wardnet/js";
import { SERVICE_OPTIONS, matchServiceLabel } from "@/lib/serviceBundles";

/** Encode an endpoint as `kind:id` for the Select value. */
function encodeEndpoint(kind: ExceptionEndpointKind, id: string): string {
  return `${kind}:${id}`;
}
function decodeEndpoint(value: string): ExceptionEndpoint {
  const [kind, id] = value.split(":");
  return { kind: kind as ExceptionEndpointKind, id: id ?? "" };
}

function serviceLabel(service: ServiceSpec): string {
  // Short pill label: match a known bundle (dropping the parenthetical), else
  // a generic port count for a custom list.
  const matched = matchServiceLabel(service);
  if (matched) return matched.replace(/\s*\(.*\)$/, "");
  const count = service.type === "ports" ? service.ports.length : 0;
  return `${count} port${count === 1 ? "" : "s"}`;
}

/**
 * Cross-zone exceptions manager (issue #737). Grants one endpoint access to
 * another across an otherwise-isolated zone boundary. The headline case is the
 * one-click **casting** preset (mDNS + SSDP/DLNA + Chromecast + AirPlay,
 * bidirectional), e.g. a phone in Trusted casting to a TV in IoT.
 */
export function ZoneExceptionsCard() {
  const { data: exceptionData } = useZoneExceptions();
  const { data: zoneData } = useNetworkZones();
  const { data: deviceData } = useDevices();
  const createException = useCreateZoneException();
  const deleteException = useDeleteZoneException();

  const [formOpen, setFormOpen] = useState(false);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const zones = useMemo(() => zoneData?.zones ?? [], [zoneData]);
  const devices = useMemo(() => deviceData?.devices ?? [], [deviceData]);
  const exceptions = useMemo(
    () => exceptionData?.exceptions ?? [],
    [exceptionData],
  );

  /** Resolve an endpoint to a display label. */
  const endpointLabel = useMemo(() => {
    const zoneMap = new Map(zones.map((z) => [z.id, z.name]));
    const deviceMap = new Map(devices.map((d) => [d.id, deviceDisplayName(d)]));
    return (endpoint: ExceptionEndpoint) =>
      endpoint.kind === "zone"
        ? (zoneMap.get(endpoint.id) ?? "Unknown zone")
        : (deviceMap.get(endpoint.id) ?? "Unknown device");
  }, [zones, devices]);

  const columns = useMemo<ColumnDef<ZoneException>[]>(
    () => [
      {
        id: "from",
        header: "From",
        cell: ({ row }) => (
          <Text as="span" size="sm">
            {endpointLabel(row.original.from)}
          </Text>
        ),
      },
      {
        id: "direction",
        header: "",
        meta: { className: "w-10 text-center" },
        cell: ({ row }) => (
          <Text as="span" size="sm" className="text-ink-3">
            {row.original.bidirectional ? "↔" : "→"}
          </Text>
        ),
      },
      {
        id: "to",
        header: "To",
        cell: ({ row }) => (
          <Text as="span" size="sm">
            {endpointLabel(row.original.to)}
          </Text>
        ),
      },
      {
        id: "service",
        header: "Service",
        meta: { className: "w-28" },
        cell: ({ row }) => (
          <Pill variant="ghost">{serviceLabel(row.original.service)}</Pill>
        ),
      },
    ],
    [endpointLabel],
  );

  return (
    <Card>
      <CardHeader>
        <CardTitle>Cross-zone exceptions</CardTitle>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        <Text size="sm" className="text-ink-3">
          Allow one device or zone to reach another across an isolated boundary
          - for example, casting from your phone to a TV in the IoT zone.
        </Text>

        {formOpen && (
          <ExceptionForm
            zones={zones.map((z) => ({ id: z.id, name: z.name }))}
            devices={devices.map((d) => ({
              id: d.id,
              name: deviceDisplayName(d),
            }))}
            isSaving={createException.isPending}
            onCancel={() => setFormOpen(false)}
            onCreate={(body) =>
              createException.mutate(body, {
                onSuccess: () => setFormOpen(false),
              })
            }
          />
        )}

        <DataTable
          columns={columns}
          data={exceptions}
          emptyMessage="No cross-zone exceptions yet."
          addLabel="Add exception"
          onAdd={() => setFormOpen(true)}
          addTestId="exception-add"
          rowActionsTestId="exception-row-menu"
          rowActions={(row) => (
            <RowAction
              onSelect={() => setDeleteId(row.id)}
              destructive
              icon={<Trash2 aria-hidden />}
              testId="exception-delete"
            >
              Delete
            </RowAction>
          )}
        />
      </CardContent>

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => {
          if (!open) setDeleteId(null);
        }}
        title="Remove exception"
        description="Remove this cross-zone exception? The allowance is revoked immediately."
        confirmLabel="Remove"
        onConfirm={() => {
          if (deleteId) deleteException.mutate(deleteId);
          setDeleteId(null);
        }}
      />
    </Card>
  );
}

interface EntityOption {
  id: string;
  name: string;
}

interface ExceptionFormProps {
  zones: EntityOption[];
  devices: EntityOption[];
  isSaving: boolean;
  onCancel: () => void;
  onCreate: (body: {
    from: ExceptionEndpoint;
    to: ExceptionEndpoint;
    service: ServiceSpec;
    bidirectional: boolean;
  }) => void;
}

function ExceptionForm({
  zones,
  devices,
  isSaving,
  onCancel,
  onCreate,
}: ExceptionFormProps) {
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [serviceId, setServiceId] = useState("casting");
  const [customPorts, setCustomPorts] = useState<PortRow[]>([emptyPortRow()]);

  const validPorts = customPorts
    .map(parsePortRow)
    .filter((p): p is PortSpec => p !== null);
  const customOk = serviceId !== "custom" || validPorts.length > 0;
  const canSave = Boolean(from) && Boolean(to) && from !== to && customOk;

  function handleSubmit() {
    if (!canSave) return;
    const option = SERVICE_OPTIONS.find((o) => o.id === serviceId);
    const service: ServiceSpec =
      serviceId === "custom" || !option || option.spec === "custom"
        ? { type: "ports", ports: validPorts }
        : option.spec;
    onCreate({
      from: decodeEndpoint(from),
      to: decodeEndpoint(to),
      service,
      // Cross-zone allowances are stateful (conntrack carries the return
      // path); bidirectional matches the casting default and the common intent.
      bidirectional: true,
    });
  }

  return (
    <Card className="border-dashed">
      <CardHeader>
        <CardTitle>Add exception</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        <div className="flex gap-3">
          <EndpointField
            label="From"
            id="exception-from"
            value={from}
            onChange={setFrom}
            zones={zones}
            devices={devices}
            className="flex-1"
          />
          <EndpointField
            label="To"
            id="exception-to"
            value={to}
            onChange={setTo}
            zones={zones}
            devices={devices}
            className="flex-1"
          />
        </div>
        {from && to && from === to && (
          <Text size="xs" className="text-danger">
            Pick two different endpoints.
          </Text>
        )}

        <Field label="Service" htmlFor="exception-service">
          <Select value={serviceId} onValueChange={setServiceId}>
            <SelectTrigger
              id="exception-service"
              data-testid="exception-service"
              className="w-full"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {SERVICE_OPTIONS.map((o) => (
                <SelectItem key={o.id} value={o.id}>
                  {o.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>

        {serviceId === "custom" && (
          <CustomPortsEditor rows={customPorts} onChange={setCustomPorts} />
        )}
        {serviceId === "custom" && !customOk && (
          <Text
            size="xs"
            className="text-danger"
            data-testid="exception-port-error"
          >
            Add at least one valid port (0-65535, with end ≥ start).
          </Text>
        )}

        <Text size="xs" className="text-ink-3">
          Grants access for the selected service both ways between the two
          endpoints.
        </Text>
      </CardContent>
      <FormActions
        secondaryLabel="Cancel"
        secondaryProps={{
          type: "button",
          onClick: onCancel,
          disabled: isSaving,
        }}
        primaryLabel={isSaving ? "Saving…" : "Add exception"}
        primaryProps={{
          type: "button",
          onClick: handleSubmit,
          disabled: isSaving || !canSave,
          "data-testid": "exception-submit",
        }}
      />
    </Card>
  );
}

interface PortRow {
  proto: Proto;
  from: string;
  to: string;
}

function emptyPortRow(): PortRow {
  return { proto: "tcp", from: "", to: "" };
}

/** Parse a UI row into a PortSpec, or null if incomplete/invalid. */
function parsePortRow(row: PortRow): PortSpec | null {
  const from = Number(row.from);
  const to = row.to === "" ? from : Number(row.to);
  if (row.from === "" || !Number.isInteger(from) || from < 0 || from > 65535) {
    return null;
  }
  if (!Number.isInteger(to) || to < from || to > 65535) return null;
  return { proto: row.proto, from, to };
}

interface CustomPortsEditorProps {
  rows: PortRow[];
  onChange: (rows: PortRow[]) => void;
}

/** Repeatable proto + port-range rows for a custom service. */
function CustomPortsEditor({ rows, onChange }: CustomPortsEditorProps) {
  function update(i: number, patch: Partial<PortRow>) {
    onChange(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));
  }
  return (
    <div className="flex flex-col gap-2">
      <Text size="2xs" className="uppercase tracking-wide text-ink-3">
        Ports
      </Text>
      {rows.map((row, i) => (
        <div key={i} className="flex items-center gap-2">
          <Select
            value={row.proto}
            onValueChange={(v) => update(i, { proto: v as Proto })}
          >
            <SelectTrigger
              className="w-24"
              aria-label={`Protocol for port row ${i + 1}`}
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="tcp">TCP</SelectItem>
              <SelectItem value="udp">UDP</SelectItem>
            </SelectContent>
          </Select>
          <Input
            aria-label={`From port row ${i + 1}`}
            data-testid={`exception-port-from-${i}`}
            inputMode="numeric"
            maxLength={5}
            className="w-20"
            placeholder="from"
            value={row.from}
            onChange={(e) =>
              update(i, { from: e.target.value.replace(/\D/g, "").slice(0, 5) })
            }
          />
          <span className="text-ink-3">-</span>
          <Input
            aria-label={`To port row ${i + 1}`}
            data-testid={`exception-port-to-${i}`}
            inputMode="numeric"
            maxLength={5}
            className="w-20"
            placeholder="to"
            value={row.to}
            onChange={(e) =>
              update(i, { to: e.target.value.replace(/\D/g, "").slice(0, 5) })
            }
          />
          {rows.length > 1 && (
            <button
              type="button"
              aria-label={`Remove port row ${i + 1}`}
              data-testid={`exception-port-remove-${i}`}
              onClick={() => onChange(rows.filter((_, idx) => idx !== i))}
              className="text-ink-3 hover:text-danger"
            >
              <Trash2 aria-hidden className="size-4" />
            </button>
          )}
        </div>
      ))}
      <Button
        variant="outline"
        size="sm"
        type="button"
        data-testid="exception-port-add"
        onClick={() => onChange([...rows, emptyPortRow()])}
        className="self-end"
      >
        <PlusIcon aria-hidden className="mr-1 size-3.5" />
        Add port
      </Button>
    </div>
  );
}

interface EndpointFieldProps {
  label: string;
  id: string;
  value: string;
  onChange: (value: string) => void;
  zones: EntityOption[];
  devices: EntityOption[];
  className?: string;
}

function EndpointField({
  label,
  id,
  value,
  onChange,
  zones,
  devices,
  className,
}: EndpointFieldProps) {
  return (
    <Field label={label} htmlFor={id} className={className}>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger id={id} data-testid={id} className="w-full">
          <SelectValue placeholder="Select a zone or device" />
        </SelectTrigger>
        <SelectContent>
          {zones.map((z) => (
            <SelectItem
              key={`zone-${z.id}`}
              value={encodeEndpoint("zone", z.id)}
            >
              Zone: {z.name}
            </SelectItem>
          ))}
          {devices.map((d) => (
            <SelectItem
              key={`device-${d.id}`}
              value={encodeEndpoint("device", d.id)}
            >
              Device: {d.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </Field>
  );
}
