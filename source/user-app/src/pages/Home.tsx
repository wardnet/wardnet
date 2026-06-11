import { useState } from "react";
import { LockIcon, ShieldCheckIcon, WifiOffIcon } from "lucide-react";
import {
  ApiErrorAlert,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  DeviceIcon,
  Pill,
  RoutingSelector,
  countryFlag,
  tunnelStatusLabel,
  tunnelStatusVariant,
  useMyDevice,
  useSetMyRule,
} from "@wardnet/web";
import type { RoutingTarget, TunnelSummary } from "@wardnet/js";

/** Two routing targets are equivalent when they resolve to the same upstream:
 *  `default`/`direct` both mean "no VPN", and two tunnels match on id. Used to
 *  enable the Save button only when the selection actually differs. */
function targetsEqual(
  a: RoutingTarget | null,
  b: RoutingTarget | null,
): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  if (a.type !== b.type) return false;
  if (a.type === "tunnel" && b.type === "tunnel")
    return a.tunnel_id === b.tunnel_id;
  return true;
}

/** Human-readable label for the admin-locked read-only view. */
function routingLabel(
  target: RoutingTarget | null,
  tunnels: TunnelSummary[],
): string {
  if (!target || target.type === "default" || target.type === "direct") {
    return "Direct (no VPN)";
  }
  if (target.type === "tunnel") {
    const t = tunnels.find((tun) => tun.id === target.tunnel_id);
    if (t) {
      const flag = t.country_code ? countryFlag(t.country_code) : "";
      return `VPN: ${flag} ${t.label}`.trim();
    }
    return "VPN";
  }
  return "Direct (no VPN)";
}

/** Editable routing control. `useSetMyRule` already raises the success/error
 *  sonner toast and invalidates the `me` query, so saving is just
 *  `mutateAsync`; the inline `ApiErrorAlert` surfaces the detail. */
function RoutingForm({
  currentRule,
  tunnels,
}: {
  currentRule: RoutingTarget | null;
  tunnels: TunnelSummary[];
}) {
  const setMyRule = useSetMyRule();

  const [target, setTarget] = useState<RoutingTarget>(
    currentRule?.type === "tunnel" ? currentRule : { type: "direct" },
  );

  const hasChanges = !targetsEqual(target, currentRule);

  async function handleSave() {
    await setMyRule.mutateAsync(target);
  }

  return (
    <div className="flex flex-col gap-4">
      <RoutingSelector value={target} onChange={setTarget} tunnels={tunnels} />

      {setMyRule.isError && (
        <ApiErrorAlert
          error={setMyRule.error}
          fallback="Failed to update routing"
        />
      )}

      <Button
        onClick={handleSave}
        disabled={!hasChanges || setMyRule.isPending}
        className="w-full"
      >
        {setMyRule.isPending ? "Saving…" : "Save"}
      </Button>
    </div>
  );
}

/**
 * Home tab — device identity + self-service routing control.
 *
 * `useMyDevice` is the unauthenticated, device-keyed endpoint (there is no
 * login in the User PWA — identity is the device on the LAN). When the caller
 * can't be matched to a device we show the not-detected fallback. Otherwise the
 * device picks its own internet routing, unless an admin has locked the rule —
 * in which case the current target is shown read-only with a lock and an
 * explanation. Ported from the admin-site `MyDevice` page into this mobile-first
 * layout; the routing logic (`targetsEqual` / `routingLabel`) is shared.
 */
export default function Home() {
  const { data, isLoading } = useMyDevice();

  const device = data?.device;
  const currentRule = data?.current_rule ?? null;
  const adminLocked = data?.admin_locked ?? false;
  const tunnels = data?.available_tunnels ?? [];
  const activeTunnel =
    currentRule?.type === "tunnel"
      ? (tunnels.find((t) => t.id === currentRule.tunnel_id) ?? null)
      : null;

  // Remount the form when the saved rule changes so its local draft resets.
  const ruleKey =
    currentRule?.type === "tunnel"
      ? `tunnel-${currentRule.tunnel_id}`
      : String(currentRule?.type ?? "null");

  if (isLoading) {
    return <p className="p-5 text-sm text-ink-3">Loading…</p>;
  }

  if (!device) {
    return (
      <div className="flex flex-col items-center gap-4 px-5 py-16 text-center">
        <WifiOffIcon className="size-12 text-ink-3/50" />
        <h1 className="text-lg font-semibold text-ink">Device not detected</h1>
        <p className="max-w-md text-sm text-ink-3">
          Your device has not been detected on the network yet. Make sure you are
          accessing Wardnet directly from the local network. Connections through
          SSH tunnels or proxies cannot be matched to your device.
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6 p-5">
      <div className="flex items-center gap-3">
        <DeviceIcon
          type={device.device_type}
          size={28}
          className="text-ink/60"
        />
        <h1 className="text-lg font-semibold text-ink">
          {device.name ?? device.hostname ?? device.mac}
        </h1>
      </div>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>Internet route</CardTitle>
            {activeTunnel && (
              <Pill variant={tunnelStatusVariant(activeTunnel.status)}>
                {tunnelStatusLabel(activeTunnel.status)}
              </Pill>
            )}
          </div>
        </CardHeader>
        <CardContent>
          {adminLocked ? (
            <div className="flex flex-col gap-3">
              <p className="text-sm text-ink">
                {routingLabel(currentRule, tunnels)}
              </p>
              <div className="flex items-start gap-2 text-ink-3">
                <LockIcon className="mt-0.5 size-4 shrink-0" />
                <p className="text-sm">
                  Your network administrator has locked this setting, so you
                  can't change how your device connects to the internet.
                </p>
              </div>
            </div>
          ) : (
            <RoutingForm
              key={ruleKey}
              currentRule={currentRule}
              tunnels={tunnels}
            />
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Verify your route</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-start gap-3 text-ink-3">
            <ShieldCheckIcon className="mt-0.5 size-4 shrink-0" />
            <p className="text-sm">
              Confirm your VPN is working by checking the public IP and location
              your device is using right now. Coming in a future update.
            </p>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
