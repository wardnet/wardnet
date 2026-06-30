import { useRef } from "react";
import { Drawer, DrawerContent, DrawerTitle, Text } from "@wardnet/web";
import { useUpdateDevice, countryFlag } from "@wardnet/web";
import type { Device, Tunnel, RoutingTarget } from "@wardnet/js";
import { CheckIcon } from "lucide-react";

interface Props {
  device: Device | null;
  tunnels: Tunnel[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
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

function tunnelSublabel(status: Tunnel["status"]): { label: string; tone: TunnelTone } {
  switch (status) {
    case "up":           return { label: "",             tone: "ok" };
    case "down":         return { label: "Down",         tone: "danger" };
    case "connecting":   return { label: "Connecting",   tone: "neutral" };
    case "reconnecting": return { label: "Reconnecting", tone: "warn" };
  }
}

function OptionRow({ label, sublabel, sublabelTone, active, disabled, onSelect, testId }: {
  label: string;
  sublabel?: string;
  sublabelTone?: TunnelTone;
  active: boolean;
  disabled: boolean;
  onSelect: () => void;
  testId?: string;
}) {
  const sublabelClass =
    sublabelTone === "danger" ? "text-danger" :
    sublabelTone === "warn"   ? "text-warn"   : "text-ink-3";
  return (
    <button
      data-testid={testId}
      onClick={onSelect}
      disabled={disabled}
      className="flex w-full items-center gap-3 px-4 py-3.5 text-left transition-colors duration-snap active:bg-sunken disabled:pointer-events-none disabled:opacity-40"
    >
      {active
        ? <CheckIcon size={16} className="shrink-0 text-accent" />
        : <span className="size-4 shrink-0" />}
      <div className="flex min-w-0 flex-1 flex-col">
        <Text as="span" size="lg" weight="medium" className="text-ink">{label}</Text>
        {sublabel && (
          <Text as="span" size="xs" className={sublabelClass}>{sublabel}</Text>
        )}
      </div>
    </button>
  );
}

export function DeviceRoutingSheet({ device, tunnels, open, onOpenChange }: Props) {
  const updateDevice = useUpdateDevice({ successMessage: "Routing updated" });

  // Keep the last non-null device in a ref so the exit animation can complete
  // even if the parent clears selectedDevice while the sheet is animating closed.
  const latchedRef = useRef<Device | null>(null);
  if (device !== null) latchedRef.current = device;
  const activeDevice = latchedRef.current;

  const current = activeKey(activeDevice?.current_rule ?? null);
  const deviceLabel = activeDevice?.name ?? activeDevice?.hostname ?? activeDevice?.mac ?? "";

  function handleSelect(target: RoutingTarget) {
    if (!activeDevice) return;
    const id = activeDevice.id;
    updateDevice.mutate(
      { id, body: { routing_target: target } },
      { onSuccess: () => onOpenChange(false) },
    );
  }

  return (
    <Drawer open={open} onOpenChange={onOpenChange}>
      <DrawerContent side="bottom" aria-describedby={undefined}>
        <div className="mx-auto mt-3 mb-4 h-1 w-10 rounded-full bg-line" />
        <DrawerTitle className="px-4 pb-1 text-[11px] font-semibold uppercase tracking-wider text-ink-3">
          Route: {deviceLabel}
        </DrawerTitle>
        <div
          data-testid="device-routing-sheet"
          className="flex flex-col"
          style={{ paddingBottom: "max(24px, env(safe-area-inset-bottom))" }}
        >
          <OptionRow
            label="Default" sublabel="Follow gateway policy"
            active={current === "default"} disabled={updateDevice.isPending}
            onSelect={() => handleSelect({ type: "default" })}
            testId="device-routing-default"
          />
          <OptionRow
            label="Direct" sublabel="No VPN"
            active={current === "direct"} disabled={updateDevice.isPending}
            onSelect={() => handleSelect({ type: "direct" })}
            testId="device-routing-direct"
          />
          {tunnels.length > 0 && <div className="mx-4 my-2 h-px bg-line" />}
          {tunnels.map((tunnel) => {
            const { label: statusLabel, tone } = tunnelSublabel(tunnel.status);
            return (
              <OptionRow
                key={tunnel.id}
                label={`${countryFlag(tunnel.country_code)} ${tunnel.label}`}
                sublabel={statusLabel || undefined}
                sublabelTone={tone === "ok" ? undefined : tone}
                active={current === tunnel.id}
                disabled={updateDevice.isPending}
                onSelect={() => handleSelect({ type: "tunnel", tunnel_id: tunnel.id })}
              />
            );
          })}
        </div>
      </DrawerContent>
    </Drawer>
  );
}
