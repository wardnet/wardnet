import { useRef, useState } from "react";
import {
  Drawer,
  DrawerContent,
  DrawerTitle,
  Text,
  Button,
  InboundWgBetaNotice,
} from "@wardnet/web";
import {
  useInboundWgPeers,
  useAddInboundWgPeer,
  countryFlag,
} from "@wardnet/web";
import type {
  AddInboundWgPeerResponse,
  Device,
  NetworkZoneView,
  Tunnel,
  RoutingTarget,
} from "@wardnet/js";
import { CheckIcon } from "lucide-react";
import { InboundWgGrantedView } from "./InboundWgGrantedView";

interface Props {
  device: Device | null;
  tunnels: Tunnel[];
  /** Network zones the device can be reassigned to (owned by the page). */
  zones: NetworkZoneView[];
  /** True while a routing or zone change is in flight; disables the rows. */
  busy: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Apply a routing override to the selected device. */
  onSelectRoute: (target: RoutingTarget) => void;
  /** Reassign the selected device to a different zone. */
  onSelectZone: (zoneId: string) => void;
}

function activeKey(rule: RoutingTarget | null): string {
  if (!rule || rule.type === "default") return "default";
  if (rule.type === "direct") return "direct";
  if (rule.type === "tunnel") return rule.tunnel_id;
  // Unknown future rule types fall back to "default" visual state rather than
  // leaving every row un-highlighted.
  return "default";
}

type TunnelTone = "ok" | "danger" | "warn" | "neutral";

function tunnelSublabel(status: Tunnel["status"]): {
  label: string;
  tone: TunnelTone;
} {
  switch (status) {
    case "up":
      return { label: "", tone: "ok" };
    case "down":
      return { label: "Down", tone: "danger" };
    case "connecting":
      return { label: "Connecting", tone: "neutral" };
    case "reconnecting":
      return { label: "Reconnecting", tone: "warn" };
  }
}

function OptionRow({
  label,
  sublabel,
  sublabelTone,
  active,
  disabled,
  onSelect,
  testId,
}: {
  label: string;
  sublabel?: string;
  sublabelTone?: TunnelTone;
  active: boolean;
  disabled: boolean;
  onSelect: () => void;
  testId?: string;
}) {
  const sublabelClass =
    sublabelTone === "danger"
      ? "text-danger"
      : sublabelTone === "warn"
        ? "text-warn"
        : "text-ink-3";
  return (
    <button
      data-testid={testId}
      onClick={onSelect}
      disabled={disabled}
      className="flex w-full items-center gap-3 px-4 py-3.5 text-left transition-colors duration-snap active:bg-sunken disabled:pointer-events-none disabled:opacity-40"
    >
      {active ? (
        <CheckIcon size={16} className="shrink-0 text-accent" />
      ) : (
        <span className="size-4 shrink-0" />
      )}
      <div className="flex min-w-0 flex-1 flex-col">
        <Text as="span" size="lg" weight="medium" className="text-ink">
          {label}
        </Text>
        {sublabel && (
          <Text as="span" size="xs" className={sublabelClass}>
            {sublabel}
          </Text>
        )}
      </div>
    </button>
  );
}

export function DeviceRoutingSheet({
  device,
  tunnels,
  zones,
  busy,
  open,
  onOpenChange,
  onSelectRoute,
  onSelectZone,
}: Props) {
  const { data: peersData } = useInboundWgPeers();
  const addPeer = useAddInboundWgPeer();

  // The one-time grant response. When set, the sheet swaps to the QR view so
  // the private key (embedded in `client_config`) can be scanned once.
  const [granted, setGranted] = useState<AddInboundWgPeerResponse | null>(null);

  // Keep the last non-null device in a ref so the exit animation can complete
  // even if the parent clears selectedDevice while the sheet is animating closed.
  const latchedRef = useRef<Device | null>(null);
  if (device !== null) latchedRef.current = device;
  const activeDevice = latchedRef.current;

  const current = activeKey(activeDevice?.current_rule ?? null);
  const deviceLabel =
    activeDevice?.name ?? activeDevice?.hostname ?? activeDevice?.mac ?? "";

  // Grantable = managed (admin-named; the backend rejects unmanaged devices)
  // AND has no peer row yet. `connection_mode` is monitor-driven liveness, not
  // credential existence, so a peer-row lookup is the authoritative signal.
  const peers = peersData?.peers ?? [];
  const alreadyGranted =
    activeDevice != null && peers.some((p) => p.device_id === activeDevice.id);
  const grantable =
    activeDevice != null && activeDevice.name != null && !alreadyGranted;

  function handleOpenChange(next: boolean) {
    if (!next) setGranted(null);
    onOpenChange(next);
  }

  function handleSelect(target: RoutingTarget) {
    if (!activeDevice) return;
    onSelectRoute(target);
  }

  function handleZoneSelect(zoneId: string) {
    if (!activeDevice || zoneId === activeDevice.zone_id) return;
    onSelectZone(zoneId);
  }

  async function handleGrant() {
    if (!activeDevice) return;
    try {
      const response = await addPeer.mutateAsync({
        device_id: activeDevice.id,
      });
      setGranted(response);
    } catch {
      // The mutation's onError already surfaced a toast; keep the sheet open so
      // the admin can retry, and swallow the rejection so it isn't an unhandled
      // promise rejection.
    }
  }

  return (
    <Drawer open={open} onOpenChange={handleOpenChange}>
      <DrawerContent side="bottom" aria-describedby={undefined}>
        <div className="mx-auto mt-3 mb-4 h-1 w-10 rounded-full bg-line" />
        <DrawerTitle className="px-4 pb-1 text-[11px] font-semibold uppercase tracking-wider text-ink-3">
          {granted
            ? `Remote access granted - ${granted.name}`
            : `Route: ${deviceLabel}`}
        </DrawerTitle>
        {granted ? (
          <div
            data-testid="device-routing-sheet"
            className="flex flex-col"
            style={{ paddingBottom: "max(24px, env(safe-area-inset-bottom))" }}
          >
            <InboundWgGrantedView
              peer={granted}
              onDone={() => handleOpenChange(false)}
            />
          </div>
        ) : (
          <div
            data-testid="device-routing-sheet"
            className="flex flex-col"
            style={{ paddingBottom: "max(24px, env(safe-area-inset-bottom))" }}
          >
            <OptionRow
              label="Default"
              sublabel="Follow gateway policy"
              active={current === "default"}
              disabled={busy}
              onSelect={() => handleSelect({ type: "default" })}
              testId="device-routing-default"
            />
            <OptionRow
              label="Direct"
              sublabel="No VPN"
              active={current === "direct"}
              disabled={busy}
              onSelect={() => handleSelect({ type: "direct" })}
              testId="device-routing-direct"
            />
            {tunnels.length > 0 && <div className="mx-4 my-2 h-px bg-line" />}
            {tunnels.map((tunnel) => {
              const { label: statusLabel, tone } = tunnelSublabel(
                tunnel.status,
              );
              return (
                <OptionRow
                  key={tunnel.id}
                  label={`${countryFlag(tunnel.country_code)} ${tunnel.label}`}
                  sublabel={statusLabel || undefined}
                  sublabelTone={tone === "ok" ? undefined : tone}
                  active={current === tunnel.id}
                  disabled={busy}
                  onSelect={() =>
                    handleSelect({ type: "tunnel", tunnel_id: tunnel.id })
                  }
                />
              );
            })}

            {zones.length > 0 && (
              <>
                <div className="mx-4 my-2 h-px bg-line" />
                <Text
                  as="p"
                  size="xs"
                  weight="medium"
                  className="px-4 pt-1 pb-0.5 uppercase tracking-wider text-ink-3"
                >
                  Zone
                </Text>
                {zones.map((zone) => (
                  <OptionRow
                    key={zone.id}
                    label={zone.name}
                    sublabel={zone.is_default ? "Home" : undefined}
                    active={activeDevice?.zone_id === zone.id}
                    disabled={busy}
                    onSelect={() => handleZoneSelect(zone.id)}
                    testId="device-zone-option"
                  />
                ))}
              </>
            )}

            {grantable && (
              <>
                <div className="mx-4 my-2 h-px bg-line" />
                <Text
                  as="p"
                  size="xs"
                  weight="medium"
                  className="px-4 pt-1 pb-1 uppercase tracking-wider text-ink-3"
                >
                  Remote access
                </Text>
                <InboundWgBetaNotice className="mx-4 mb-2" />
                <div className="px-4 pt-1">
                  <Button
                    className="w-full"
                    data-testid="device-grant-remote-access"
                    onClick={handleGrant}
                    disabled={addPeer.isPending}
                  >
                    {addPeer.isPending ? "Granting…" : "Grant remote access"}
                  </Button>
                  <Text as="p" size="xs" className="pt-2 text-ink-3">
                    Issues a WireGuard credential so this device can connect
                    back in from off the LAN. You&apos;ll get a QR code to scan
                    once.
                  </Text>
                </div>
              </>
            )}
          </div>
        )}
      </DrawerContent>
    </Drawer>
  );
}
