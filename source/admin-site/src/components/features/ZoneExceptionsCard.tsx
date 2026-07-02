import { useMemo, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Trash2 } from "lucide-react";
import { FormActions } from "@wardnet/web";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Field } from "@wardnet/web";
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
  ServiceSpec,
  ZoneException,
} from "@wardnet/js";

/** Encode an endpoint as `kind:id` for the Select value. */
function encodeEndpoint(kind: ExceptionEndpointKind, id: string): string {
  return `${kind}:${id}`;
}
function decodeEndpoint(value: string): ExceptionEndpoint {
  const [kind, id] = value.split(":");
  return { kind: kind as ExceptionEndpointKind, id: id ?? "" };
}

function serviceLabel(service: ServiceSpec): string {
  if (service.type === "preset") return "Casting";
  const count = service.ports.length;
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
          — for example, casting from your phone to a TV in the IoT zone.
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

  const canSave = from && to && from !== to;

  function handleSubmit() {
    if (!canSave) return;
    // Only the casting preset is offered here — it's the one intent this
    // surface exists for. Custom port lists are a power-user concern the
    // daemon still supports via the API.
    onCreate({
      from: decodeEndpoint(from),
      to: decodeEndpoint(to),
      service: { type: "preset", set: "casting" },
      bidirectional: true,
    });
  }

  return (
    <Card className="border-dashed">
      <CardHeader>
        <CardTitle>Allow casting</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        <EndpointField
          label="From"
          id="exception-from"
          value={from}
          onChange={setFrom}
          zones={zones}
          devices={devices}
        />
        <EndpointField
          label="To"
          id="exception-to"
          value={to}
          onChange={setTo}
          zones={zones}
          devices={devices}
        />
        {from && to && from === to && (
          <Text size="xs" className="text-danger">
            Pick two different endpoints.
          </Text>
        )}
        <Text size="xs" className="text-ink-3">
          Opens the casting ports (mDNS, DLNA, Chromecast, AirPlay) both ways
          between the two endpoints.
        </Text>
      </CardContent>
      <FormActions
        secondaryLabel="Cancel"
        secondaryProps={{
          type: "button",
          onClick: onCancel,
          disabled: isSaving,
        }}
        primaryLabel={isSaving ? "Saving…" : "Allow casting"}
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

interface EndpointFieldProps {
  label: string;
  id: string;
  value: string;
  onChange: (value: string) => void;
  zones: EntityOption[];
  devices: EntityOption[];
}

function EndpointField({
  label,
  id,
  value,
  onChange,
  zones,
  devices,
}: EndpointFieldProps) {
  return (
    <Field label={label} htmlFor={id}>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger id={id} className="w-full sm:w-80">
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
