import { useCallback, useMemo, useState } from "react";
import { CheckCircle2Icon, TriangleAlertIcon, XCircleIcon } from "lucide-react";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
  Button,
  Text,
  Toggle,
  deviceDisplayName,
} from "@wardnet/web";
import type {
  Device,
  PrivateDnsGrantSummary,
  PrivateDnsPrerequisites,
  PrivateDnsStatusResponse,
} from "@wardnet/js";
import type { MutationHandle } from "@/lib/mutationHandle";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import { DeviceSelect } from "@/components/compound/DeviceSelect";
import { PrivateDnsGrantsTable } from "@/components/compound/PrivateDnsGrantsTable";
import { PrivateDnsGrantedModal } from "@/components/features/PrivateDnsGrantedModal";

interface PrivateDnsCardProps {
  /** The Private DNS status, or `undefined` while the page's query loads. */
  status: PrivateDnsStatusResponse | undefined;
  isLoading: boolean;
  devices: Device[];
  onSetEnabled: (enabled: boolean) => void;
  /** True while the page's enable/disable mutation is in flight. */
  setEnabledPending: boolean;
  /** The page's hoisted grant mutation. The card consumes the resolved grant
   *  to open the setup-link modal. */
  grantDevice: MutationHandle<string, PrivateDnsGrantSummary>;
  onRevokeDevice: (deviceId: string) => void;
  onSendToDevice: (deviceId: string) => void;
  /** True while the page's send-to-device mutation is in flight. */
  sendPending: boolean;
}

/**
 * Private DNS (issue #915) — always mounted on the Remote Access page so the
 * prerequisite checklist explains what's missing even before a Wardnet hostname
 * exists. Enabling is gated on three hard prerequisites (wardnet provider, live
 * certificate, Premium entitlement); the reverse tunnel is informational only.
 * Once enabled the admin grants managed devices and can push a setup link to
 * each granted device. Pure presentation — the owning page wires the
 * query/mutation hooks and passes data + callbacks in.
 */
export function PrivateDnsCard({
  status,
  isLoading,
  devices,
  onSetEnabled,
  setEnabledPending,
  grantDevice,
  onRevokeDevice,
  onSendToDevice,
  sendPending,
}: PrivateDnsCardProps) {
  const deviceById = useMemo(
    () => new Map(devices.map((d) => [d.id, d])),
    [devices],
  );
  const deviceName = useCallback(
    (deviceId: string): string => {
      const device = deviceById.get(deviceId);
      return device
        ? deviceDisplayName(device, device.last_ip)
        : "Unknown device";
    },
    [deviceById],
  );

  // Memoised so the `?? []` fallback can't mint a fresh identity every render
  // and defeat the `grantable` memo below.
  const grants = useMemo(() => status?.grants ?? [], [status?.grants]);
  const enabled = status?.enabled ?? false;

  // Only managed (admin-named) devices can be granted; a granted device drops
  // out of the picker so the one-grant-per-device guard is never hit.
  const grantable = useMemo(() => {
    const grantedIds = new Set(grants.map((g) => g.device_id));
    return devices.filter((d) => d.name != null && !grantedIds.has(d.id));
  }, [devices, grants]);

  const [creating, setCreating] = useState(false);
  const [selectedDeviceId, setSelectedDeviceId] = useState("");
  const [granted, setGranted] = useState<PrivateDnsGrantSummary | null>(null);
  const [revokeTarget, setRevokeTarget] =
    useState<PrivateDnsGrantSummary | null>(null);

  const hardMet = status
    ? status.prerequisites.wardnet_provider &&
      status.prerequisites.certificate &&
      status.prerequisites.entitled
    : false;

  async function handleGrant() {
    if (!selectedDeviceId) return;
    try {
      const grant = await grantDevice.mutateAsync(selectedDeviceId);
      setGranted(grant);
      setCreating(false);
      setSelectedDeviceId("");
    } catch {
      // The mutation's onError already surfaced a toast; keep the panel open so
      // the admin can retry, and swallow the rejection.
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Private DNS</CardTitle>
        <CardAction>
          <Toggle
            aria-label="Enable Private DNS"
            checked={enabled}
            // Enabling needs every hard prerequisite; disabling is always
            // allowed.
            disabled={isLoading || setEnabledPending || (!enabled && !hardMet)}
            onCheckedChange={onSetEnabled}
          />
        </CardAction>
      </CardHeader>

      <CardContent className="flex flex-col gap-5">
        <Text as="p" size="sm" className="text-ink-3">
          Let phones resolve DNS through Wardnet everywhere — at home and
          roaming — with Android Private DNS or an iOS profile. No VPN required.
        </Text>

        {status && (
          <PrerequisiteChecklist prerequisites={status.prerequisites} />
        )}

        {enabled && (
          <div className="flex flex-col gap-4 border-t border-line pt-5">
            <div className="flex items-center justify-between">
              <Text as="h4" size="sm" weight="medium" className="text-ink">
                Granted devices
              </Text>
              {!creating && grantable.length > 0 && (
                <Button size="sm" onClick={() => setCreating(true)}>
                  Grant access
                </Button>
              )}
            </div>

            {devices.every((d) => d.name == null) && (
              <Text as="p" size="sm" className="text-ink-3">
                Only managed devices can be granted. Name a device on the
                Devices page, then grant it access here.
              </Text>
            )}

            {creating && (
              <div className="flex flex-col gap-3 rounded-lg border border-line p-4 sm:flex-row sm:items-end">
                <div className="flex-1">
                  <Text
                    as="p"
                    size="xs"
                    weight="medium"
                    className="mb-2 text-ink-3"
                  >
                    Device
                  </Text>
                  <DeviceSelect
                    devices={grantable}
                    value={selectedDeviceId}
                    onChange={setSelectedDeviceId}
                    valueKey="id"
                    includeAny={false}
                    anyLabel="Choose a device"
                    emptyLabel="Every managed device already has access."
                  />
                </div>
                <div className="flex gap-2">
                  <Button
                    variant="ghost"
                    onClick={() => {
                      setCreating(false);
                      setSelectedDeviceId("");
                    }}
                    disabled={grantDevice.isPending}
                  >
                    Cancel
                  </Button>
                  <Button
                    onClick={handleGrant}
                    disabled={!selectedDeviceId || grantDevice.isPending}
                  >
                    {grantDevice.isPending ? "Granting…" : "Grant"}
                  </Button>
                </div>
              </div>
            )}

            <PrivateDnsGrantsTable
              grants={grants}
              deviceName={deviceName}
              onSendToDevice={(grant) => onSendToDevice(grant.device_id)}
              onRevoke={setRevokeTarget}
            />
          </div>
        )}
      </CardContent>

      <PrivateDnsGrantedModal
        grant={granted}
        deviceName={granted ? deviceName(granted.device_id) : ""}
        onSendToDevice={(grant) => onSendToDevice(grant.device_id)}
        sending={sendPending}
        onDismiss={() => setGranted(null)}
      />

      <ConfirmDialog
        open={!!revokeTarget}
        onOpenChange={(open) => {
          if (!open) setRevokeTarget(null);
        }}
        title="Revoke Private DNS access"
        description={`This cuts off ${
          revokeTarget ? deviceName(revokeTarget.device_id) : "this device"
        } and deletes its hostname. Re-granting later mints a new one.`}
        confirmLabel="Revoke"
        onConfirm={() => {
          if (revokeTarget) onRevokeDevice(revokeTarget.device_id);
          setRevokeTarget(null);
        }}
      />
    </Card>
  );
}

const HARD_ROWS: {
  key: keyof PrivateDnsPrerequisites;
  label: string;
  unmetHint: string;
}[] = [
  {
    key: "wardnet_provider",
    label: "Wardnet hostname",
    unmetHint:
      "Requires a Wardnet hostname — set one up under Remote access above.",
  },
  {
    key: "certificate",
    label: "HTTPS certificate",
    unmetHint: "No live certificate yet.",
  },
  {
    key: "entitled",
    label: "Premium subscription",
    unmetHint: "Private DNS is a Premium feature.",
  },
];

/** The three hard gates plus the informational reverse-tunnel row. Reuses the
 *  Remote Access tone conventions (accent = met, danger = unmet, warning =
 *  informational). */
function PrerequisiteChecklist({
  prerequisites,
}: {
  prerequisites: PrivateDnsPrerequisites;
}) {
  return (
    <div
      className="flex flex-col gap-2"
      data-testid="private-dns-prerequisites"
    >
      {HARD_ROWS.map((row) => (
        <PrereqRow
          key={row.key}
          label={row.label}
          met={prerequisites[row.key]}
          hint={prerequisites[row.key] ? undefined : row.unmetHint}
        />
      ))}
      <PrereqRow
        label="Reverse tunnel"
        met={prerequisites.relay_connected}
        informational
        hint={
          prerequisites.relay_connected
            ? undefined
            : "Not connected — the home network still works; roaming needs this."
        }
      />
    </div>
  );
}

function PrereqRow({
  label,
  met,
  hint,
  informational = false,
}: {
  label: string;
  met: boolean;
  hint?: string;
  informational?: boolean;
}) {
  const tone = met
    ? "text-accent-soft-ink"
    : informational
      ? "text-warning"
      : "text-danger-soft-ink";
  const Icon = met
    ? CheckCircle2Icon
    : informational
      ? TriangleAlertIcon
      : XCircleIcon;

  return (
    <div className="flex items-start gap-2">
      <Icon size={16} className={`mt-0.5 shrink-0 ${tone}`} aria-hidden />
      <div className="flex flex-col">
        <Text as="span" size="sm" weight="medium" className="text-ink">
          {label}
        </Text>
        {hint && (
          <Text as="span" size="xs" className="text-ink-3">
            {hint}
          </Text>
        )}
      </div>
    </div>
  );
}
