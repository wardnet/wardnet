import { useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import type { ColumnDef } from "@tanstack/react-table";
import { Button } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { ApiErrorAlert } from "@wardnet/web";
import { FormActions } from "@wardnet/web";
import { RoutingSelector } from "@wardnet/web";
import { DeviceIcon } from "@wardnet/web";
import { deviceDisplayName } from "@wardnet/web";
import { sortByLabel } from "@wardnet/web";
import { DetailPageHeader } from "@/components/compound/DetailPageHeader";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import { HostCell } from "@/components/compound/HostCell";
import { DataTable } from "@/components/core/ui/data-table";
import {
  useRoutingProfile,
  useDomainRoutingRules,
  useCreateDomainRoutingRule,
  useUpdateDomainRoutingRule,
  useDeleteDomainRoutingRule,
  useProfileDevices,
  useTunnels,
  useDevices,
  countryFlag,
} from "@wardnet/web";
import type {
  Device,
  DomainRoutingRule,
  DomainRoutingTarget,
  RoutingTarget,
  Tunnel,
} from "@wardnet/js";

/** Human label for a rule's routing target. */
function targetLabel(target: DomainRoutingTarget, tunnels: Tunnel[]): string {
  if (target.type === "direct") return "Direct (no VPN)";
  const tunnel = tunnels.find((t) => t.id === target.tunnel_id);
  if (!tunnel) return "Via tunnel";
  const flag = tunnel.country_code
    ? `${countryFlag(tunnel.country_code)} `
    : "";
  return `${flag}${tunnel.label}`;
}

/** Profile detail page: rules editor + the read-only "used by" device list. */
export default function RoutingProfileDetail() {
  const { id = "" } = useParams<{ id: string }>();
  const { data, isLoading, isError } = useRoutingProfile(id);
  const profile = data?.profile;

  if (isLoading) {
    return (
      <div className="p-6">
        <Text as="p" size="sm" className="text-ink-3">
          Loading…
        </Text>
      </div>
    );
  }

  if (isError || !profile) {
    return (
      <div className="flex flex-col gap-2 p-6">
        <Text as="h1" size="xl" weight="semibold">
          Profile not found
        </Text>
        <Link to="/routing" className="text-sm text-accent underline">
          Back to Routing
        </Link>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6 p-4 md:p-6">
      <DetailPageHeader
        parentLabel="Routing"
        parentTo="/routing"
        itemLabel={profile.name}
      />
      <RulesCard profileId={profile.id} />
      <UsedByCard profileId={profile.id} />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

function RulesCard({ profileId }: { profileId: string }) {
  const { data } = useDomainRoutingRules(profileId);
  const { data: tunnelData } = useTunnels();
  const tunnels = tunnelData?.tunnels ?? [];

  const create = useCreateDomainRoutingRule();
  const update = useUpdateDomainRoutingRule();
  const remove = useDeleteDomainRoutingRule();

  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<DomainRoutingRule | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const rules = data?.rules ?? [];
  const toDelete = rules.find((r) => r.id === deleteId);

  function startAdd() {
    setEditing(null);
    setAdding(true);
  }

  async function handleDelete() {
    if (!deleteId) return;
    await remove.mutateAsync(deleteId);
    setDeleteId(null);
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Domain rules</CardTitle>
        {!adding && !editing && (
          <CardAction>
            <Button
              variant="outline"
              size="sm"
              onClick={startAdd}
              data-testid="routing-rule-add"
            >
              Add rule
            </Button>
          </CardAction>
        )}
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        {adding && (
          <RuleForm
            tunnels={tunnels}
            pending={create.isPending}
            error={create.error}
            onCancel={() => setAdding(false)}
            onSubmit={async (body) => {
              await create.mutateAsync({ profileId, body });
              setAdding(false);
            }}
          />
        )}

        {rules.length === 0 && !adding && (
          <Text size="sm" className="py-4 text-center text-ink-3">
            No rules yet. Add one to route a domain through a tunnel or direct.
          </Text>
        )}

        <div className="flex flex-col divide-y divide-line">
          {rules.map((rule) =>
            editing?.id === rule.id ? (
              <div key={rule.id} className="py-3">
                <RuleForm
                  initial={rule}
                  tunnels={tunnels}
                  pending={update.isPending}
                  error={update.error}
                  onCancel={() => setEditing(null)}
                  onSubmit={async (body) => {
                    await update.mutateAsync({ ruleId: rule.id, body });
                    setEditing(null);
                  }}
                />
              </div>
            ) : (
              <div
                key={rule.id}
                data-testid="routing-rule-row"
                className="flex items-center gap-3 py-3"
              >
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <Text
                    weight="medium"
                    className="truncate"
                    title={rule.pattern}
                  >
                    {rule.pattern}
                  </Text>
                  <Text size="xs" className="text-ink-3">
                    {targetLabel(rule.target, tunnels)}
                  </Text>
                </div>
                <Toggle
                  aria-label={`Enable rule ${rule.pattern}`}
                  checked={rule.enabled}
                  disabled={update.isPending}
                  onCheckedChange={(next) =>
                    update.mutate({ ruleId: rule.id, body: { enabled: next } })
                  }
                />
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    setAdding(false);
                    setEditing(rule);
                  }}
                  data-testid="routing-rule-edit"
                >
                  Edit
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setDeleteId(rule.id)}
                  data-testid="routing-rule-delete"
                >
                  Delete
                </Button>
              </div>
            ),
          )}
        </div>
      </CardContent>

      <ConfirmDialog
        open={deleteId !== null}
        onOpenChange={(next) => !next && setDeleteId(null)}
        title="Delete rule"
        description={
          toDelete ? `Delete the rule for "${toDelete.pattern}"?` : ""
        }
        confirmLabel="Delete"
        onConfirm={handleDelete}
      />
    </Card>
  );
}

interface RuleFormProps {
  initial?: DomainRoutingRule;
  tunnels: Tunnel[];
  pending: boolean;
  error: unknown;
  onCancel: () => void;
  onSubmit: (body: {
    pattern: string;
    target: DomainRoutingTarget;
    enabled: boolean;
  }) => void | Promise<void>;
}

/** Inline add/edit form for a single domain rule. */
function RuleForm({
  initial,
  tunnels,
  pending,
  error,
  onCancel,
  onSubmit,
}: RuleFormProps) {
  const [pattern, setPattern] = useState(initial?.pattern ?? "");
  const [target, setTarget] = useState<DomainRoutingTarget>(
    initial?.target ?? { type: "direct" },
  );

  const trimmed = pattern.trim();
  const disabled = pending || trimmed === "";

  return (
    <div className="flex flex-col gap-4">
      <Field label="Domain pattern" htmlFor="routing-rule-pattern">
        <Input
          id="routing-rule-pattern"
          data-testid="routing-rule-pattern"
          value={pattern}
          onChange={(e) => setPattern(e.target.value)}
          placeholder="netflix.com or *.netflix.com"
        />
      </Field>
      <Field label="Route via">
        <RoutingSelector
          value={target as RoutingTarget}
          onChange={(next) => {
            // RoutingSelector only ever emits `direct` or `tunnel`, both valid
            // domain-rule targets; guard the `default` case for type safety.
            if (next.type !== "default") setTarget(next);
          }}
          tunnels={tunnels}
          isAdmin
          data-testid="routing-rule-target"
        />
      </Field>
      {error != null && (
        <ApiErrorAlert error={error} fallback="Failed to save rule" />
      )}
      <FormActions
        secondaryLabel="Cancel"
        secondaryProps={{ onClick: onCancel, disabled: pending }}
        primaryLabel={pending ? "Saving…" : initial ? "Save" : "Add rule"}
        primaryProps={{
          onClick: () =>
            void onSubmit({
              pattern: trimmed,
              target,
              enabled: initial?.enabled ?? true,
            }),
          disabled,
          "data-testid": "routing-rule-save",
        }}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Used by
// ---------------------------------------------------------------------------

/** Device + IP columns, matching the tunnel "used by" table
 *  (`TunnelDevicesTable`) so device rows read consistently across the app. */
function buildUsedByColumns(): ColumnDef<Device>[] {
  return [
    {
      accessorKey: "name",
      header: "Device",
      cell: ({ row }) => {
        const device = row.original;
        const primary = deviceDisplayName(device);
        const secondary = primary === device.mac ? null : device.mac;
        return (
          <HostCell
            primary={primary}
            secondary={secondary}
            icon={<DeviceIcon type={device.device_type} />}
          />
        );
      },
    },
    {
      accessorKey: "last_ip",
      header: "IP",
      meta: { className: "hidden md:table-cell" },
      cell: ({ row }) => (
        <span className="text-ink-3">{row.original.last_ip}</span>
      ),
    },
  ];
}

function UsedByCard({ profileId }: { profileId: string }) {
  const navigate = useNavigate();
  const { data, isLoading } = useProfileDevices(profileId);
  const { data: deviceData } = useDevices();

  const deviceIds = data?.device_ids ?? [];
  const allDevices = deviceData?.devices ?? [];

  // Resolve assigned ids to full device records (skip any we can't find), then
  // sort by display name for a stable, readable order.
  const devices = useMemo(() => {
    const idSet = new Set(deviceIds);
    return sortByLabel(
      allDevices.filter((d) => idSet.has(d.id)),
      deviceDisplayName,
    );
  }, [deviceIds, allDevices]);

  const columns = useMemo(() => buildUsedByColumns(), []);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Used by</CardTitle>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <Text size="sm" className="text-ink-3">
            Loading…
          </Text>
        ) : devices.length === 0 ? (
          <Text size="sm" className="text-ink-3">
            No devices use this profile yet. Assign it from a device's detail
            page.
          </Text>
        ) : (
          <DataTable
            columns={columns}
            data={devices}
            onRowClick={(device) => void navigate(`/devices/${device.id}`)}
          />
        )}
      </CardContent>
    </Card>
  );
}
