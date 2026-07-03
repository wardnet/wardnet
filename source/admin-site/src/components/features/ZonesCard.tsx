import { useMemo, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Home, Pencil, Trash2, UserPlus } from "lucide-react";
import { FormActions } from "@wardnet/web";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Form, Validator } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Pill } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { SubnetInput } from "@wardnet/web";
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
  useNetworkZones,
  useCreateNetworkZone,
  useUpdateNetworkZone,
  useDeleteNetworkZone,
} from "@wardnet/web";
import type {
  AllowedTargetKind,
  NetworkZoneView,
  ZoneStance,
} from "@wardnet/js";
import { IsolationDisclaimer } from "./IsolationDisclaimer";

const STANCE_LABEL: Record<ZoneStance, string> = {
  shared_subnet: "Shared subnet",
  isolate_members: "Isolate members",
};

/** The Network Zones management surface: full lifecycle + promotion. */
export function ZonesCard() {
  const { data } = useNetworkZones();
  const createZone = useCreateNetworkZone();
  const updateZone = useUpdateNetworkZone();
  const setHome = useUpdateNetworkZone({ successMessage: "Home zone updated" });
  const setDefaultForNew = useUpdateNetworkZone({
    successMessage: "Default-for-new zone updated",
  });
  const deleteZone = useDeleteNetworkZone();

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<NetworkZoneView | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const zones = useMemo(() => data?.zones ?? [], [data]);
  const isSaving = createZone.isPending || updateZone.isPending;
  const zoneToDelete = zones.find((z) => z.id === deleteId);

  function openCreate() {
    setEditing(null);
    setFormOpen(true);
  }
  function openEdit(zone: NetworkZoneView) {
    setEditing(zone);
    setFormOpen(true);
  }
  function closeForm() {
    setFormOpen(false);
    setEditing(null);
  }

  const columns = useMemo<ColumnDef<NetworkZoneView>[]>(
    () => [
      {
        id: "name",
        header: "Zone",
        cell: ({ row }) => (
          <span className="flex flex-wrap items-center gap-2">
            <Text as="span" size="sm" className="font-medium">
              {row.original.name}
            </Text>
            {row.original.is_default && (
              <span title="The home zone — your main trusted network. Full trust, exactly one, can't be deleted.">
                <Pill variant="ghost">Home</Pill>
              </span>
            )}
            {row.original.is_default_for_new && (
              <span title="Newly-discovered devices land here until an admin moves them.">
                <Pill variant="ghost">New devices</Pill>
              </span>
            )}
            {row.original.provenance === "system" && (
              <span title="Built-in zone seeded by Wardnet — can't be deleted (unlike zones you create).">
                <Pill variant="ghost">System</Pill>
              </span>
            )}
          </span>
        ),
      },
      {
        id: "stance",
        header: "Isolation",
        meta: { className: "hidden sm:table-cell" },
        cell: ({ row }) => (
          <Text as="span" size="sm">
            {STANCE_LABEL[row.original.isolation_stance]}
            {row.original.member_isolation && row.original.subnet
              ? " · members"
              : ""}
          </Text>
        ),
      },
      {
        id: "targets",
        header: "Routing",
        meta: { className: "hidden md:table-cell" },
        cell: ({ row }) => (
          <Text as="span" size="sm" className="capitalize">
            {row.original.allowed_targets.join(", ") || "—"}
          </Text>
        ),
      },
      {
        id: "subnet",
        header: "Subnet",
        meta: { className: "hidden lg:table-cell" },
        cell: ({ row }) => (
          <Text as="span" size="xs" className="font-mono">
            {row.original.subnet?.cidr ?? "base LAN"}
          </Text>
        ),
      },
      {
        id: "members",
        header: "Devices",
        meta: { className: "w-20" },
        cell: ({ row }) => (
          <Text as="span" size="sm">
            {row.original.member_count}
          </Text>
        ),
      },
    ],
    [],
  );

  return (
    <Card>
      <CardHeader>
        <CardTitle>Zones</CardTitle>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        <IsolationDisclaimer />

        {formOpen && (
          <ZoneForm
            key={editing?.id ?? "new"}
            zone={editing}
            isSaving={isSaving}
            onCancel={closeForm}
            onCreate={(body) =>
              createZone.mutate(body, { onSuccess: closeForm })
            }
            onUpdate={(id, body) =>
              updateZone.mutate({ id, body }, { onSuccess: closeForm })
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
              {!row.is_default && (
                <RowAction
                  onSelect={() =>
                    setHome.mutate({ id: row.id, body: { is_default: true } })
                  }
                  icon={<Home aria-hidden />}
                  testId="zone-set-home"
                >
                  Set as home
                </RowAction>
              )}
              {!row.is_default_for_new && (
                <RowAction
                  onSelect={() =>
                    setDefaultForNew.mutate({
                      id: row.id,
                      body: { is_default_for_new: true },
                    })
                  }
                  icon={<UserPlus aria-hidden />}
                  testId="zone-set-default-for-new"
                >
                  Default for new devices
                </RowAction>
              )}
              {row.provenance !== "system" &&
                !row.is_default &&
                row.member_count === 0 && (
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
        description={`Delete the "${zoneToDelete?.name ?? "this"}" zone? This can't be undone.`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteId) deleteZone.mutate(deleteId);
          setDeleteId(null);
        }}
      />
    </Card>
  );
}

interface ZoneFormBody {
  name: string;
  isolation_stance: ZoneStance;
  allowed_targets: AllowedTargetKind[];
  member_isolation: boolean;
  admin_ui_reachable: boolean;
  subnet: { cidr: string } | null;
}

interface ZoneFormProps {
  zone: NetworkZoneView | null;
  isSaving: boolean;
  onCancel: () => void;
  onCreate: (body: ZoneFormBody) => void;
  onUpdate: (id: string, body: ZoneFormBody) => void;
}

function ZoneForm({
  zone,
  isSaving,
  onCancel,
  onCreate,
  onUpdate,
}: ZoneFormProps) {
  const [name, setName] = useState(zone?.name ?? "");
  const [stance, setStance] = useState<ZoneStance>(
    zone?.isolation_stance ?? "shared_subnet",
  );
  const [allowDirect, setAllowDirect] = useState(
    zone ? zone.allowed_targets.includes("direct") : true,
  );
  const [allowTunnel, setAllowTunnel] = useState(
    zone ? zone.allowed_targets.includes("tunnel") : true,
  );
  const [memberIsolation, setMemberIsolation] = useState(
    zone?.member_isolation ?? false,
  );
  const [adminUiReachable, setAdminUiReachable] = useState(
    zone?.admin_ui_reachable ?? true,
  );
  const [subnet, setSubnet] = useState(zone?.subnet?.cidr ?? "");

  // At least one routing target must be allowed (the daemon rejects empty).
  const noTarget = !allowDirect && !allowTunnel;
  // Member isolation only has any effect once the zone owns a subnet, so the
  // toggle is gated on one being set (see the isolation model in CONTEXT.md).
  const hasSubnet = subnet.trim().length > 0;

  function handleSave(values: { name: string }) {
    const allowed_targets: AllowedTargetKind[] = [];
    if (allowDirect) allowed_targets.push("direct");
    if (allowTunnel) allowed_targets.push("tunnel");
    if (allowed_targets.length === 0) return;

    const trimmedSubnet = subnet.trim();
    const body: ZoneFormBody = {
      name: values.name.trim(),
      isolation_stance: stance,
      allowed_targets,
      // Never persist an inert member-isolation flag on a subnet-less zone.
      member_isolation: trimmedSubnet ? memberIsolation : false,
      admin_ui_reachable: adminUiReachable,
      subnet: trimmedSubnet ? { cidr: trimmedSubnet } : null,
    };
    if (zone) {
      onUpdate(zone.id, body);
    } else {
      onCreate(body);
    }
  }

  return (
    <Card className="border-dashed">
      <CardHeader>
        <CardTitle>{zone ? "Edit zone" : "Add zone"}</CardTitle>
      </CardHeader>
      <Form values={{ name }} onSubmit={handleSave}>
        <CardContent className="flex flex-col gap-5">
          <div className="flex flex-col gap-5 sm:flex-row">
            <Field
              label="Name"
              htmlFor="zone-name"
              name="name"
              className="flex-1"
            >
              <Input
                id="zone-name"
                data-testid="zone-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Guest"
              />
            </Field>
            <Validator
              name="name"
              rule="required"
              message="Name is required."
            />

            <Field
              label="Isolation stance"
              htmlFor="zone-stance"
              className="sm:w-56"
            >
              <Select
                value={stance}
                onValueChange={(v) => setStance(v as ZoneStance)}
              >
                <SelectTrigger id="zone-stance">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="shared_subnet">Shared subnet</SelectItem>
                  <SelectItem value="isolate_members">
                    Isolate members
                  </SelectItem>
                </SelectContent>
              </Select>
            </Field>
          </div>

          <Field
            label="Allowed routing"
            help="Which routing targets a device in this zone may pick. At least one is required."
          >
            <div className="flex flex-col gap-2">
              <label className="flex items-center justify-between">
                <Text size="sm">Direct (no VPN)</Text>
                <Toggle
                  aria-label="Allow direct routing"
                  checked={allowDirect}
                  onCheckedChange={setAllowDirect}
                />
              </label>
              <label className="flex items-center justify-between">
                <Text size="sm">Tunnel (VPN)</Text>
                <Toggle
                  aria-label="Allow tunnel routing"
                  checked={allowTunnel}
                  onCheckedChange={setAllowTunnel}
                />
              </label>
              {noTarget && (
                <Text size="xs" className="text-danger">
                  Allow at least one routing target.
                </Text>
              )}
            </div>
          </Field>

          <Field
            label="Zone subnet"
            help="Optional. Gives the zone its own address space for cross-subnet isolation. Requires Wardnet to be the DHCP server; recorded but inactive otherwise."
          >
            <SubnetInput
              value={subnet}
              onChange={setSubnet}
              testId="zone-subnet"
            />
          </Field>

          <label className="flex items-center justify-between">
            <span className="flex flex-col">
              <Text size="sm">Member isolation</Text>
              <Text size="xs" className="text-ink-3">
                {hasSubnet
                  ? "Isolate same-zone peers from each other (requires Wardnet DHCP)."
                  : "Set a zone subnet above to enable — has no effect without one."}
              </Text>
            </span>
            <Toggle
              aria-label="Member isolation"
              checked={hasSubnet && memberIsolation}
              disabled={!hasSubnet}
              onCheckedChange={setMemberIsolation}
            />
          </label>

          <label className="flex items-center justify-between">
            <span className="flex flex-col">
              <Text size="sm">Admin UI reachable</Text>
              <Text size="xs" className="text-ink-3">
                May devices in this zone reach the Pi's admin surfaces?
              </Text>
            </span>
            <Toggle
              aria-label="Admin UI reachable"
              checked={adminUiReachable}
              onCheckedChange={setAdminUiReachable}
            />
          </label>
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
            disabled: isSaving || noTarget,
            "data-testid": "zone-submit",
          }}
        />
      </Form>
    </Card>
  );
}
