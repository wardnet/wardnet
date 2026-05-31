import { useState } from "react";
import { Button } from "@wardnet/forge-web/button";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { RoutingSelector } from "@/components/compound/RoutingSelector";
import { useMyDevice, useSetMyRule } from "@wardnet/wardnet-web";
import { countryFlag } from "@wardnet/wardnet-web";
import type { RoutingTarget, TunnelSummary } from "@wardnet/js";
import { LockIcon, WifiOffIcon } from "lucide-react";

function targetsEqual(a: RoutingTarget | null, b: RoutingTarget | null): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  if (a.type !== b.type) return false;
  if (a.type === "tunnel" && b.type === "tunnel") return a.tunnel_id === b.tunnel_id;
  return true;
}

function routingLabel(target: RoutingTarget | null, tunnels: TunnelSummary[]): string {
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
    <div className="col gap-4">
      <RoutingSelector value={target} onChange={setTarget} tunnels={tunnels} />

      {setMyRule.isError && (
        <ApiErrorAlert error={setMyRule.error} fallback="Failed to update routing" />
      )}

      <Button onClick={handleSave} disabled={!hasChanges || setMyRule.isPending} className="w-full">
        {setMyRule.isPending ? "Saving…" : "Save"}
      </Button>
    </div>
  );
}

/** Self-service per-device page: shows the caller's device identity and lets
 *  the user pick a routing target (or shows the admin-locked read-only
 *  state). Page wrapper is Forge `col gap-20` to match the section rhythm
 *  used by `DeviceDetail` / `TunnelDetail`; loading and not-detected fall
 *  back to `.h-title` / `.h-sub` per the slice T6-β precedent. Compounds
 *  (`DeviceIcon`, `RoutingSelector`, `ApiErrorAlert`) are already ported,
 *  Forge `Card` / `Button` are used directly. Public API unchanged —
 *  default export, no props (consumed via `<Route element={<MyDevice />} />`
 *  in `App.tsx`). */
export default function MyDevice() {
  const { data, isLoading } = useMyDevice();

  const device = data?.device;
  const currentRule = data?.current_rule ?? null;
  const adminLocked = data?.admin_locked ?? false;
  const tunnels = data?.available_tunnels ?? [];

  const ruleKey =
    currentRule?.type === "tunnel"
      ? `tunnel-${currentRule.tunnel_id}`
      : String(currentRule?.type ?? "null");

  if (isLoading) {
    return <p className="text-sm text-ink-3">Loading…</p>;
  }

  if (!device) {
    return (
      <div className="col items-center gap-4 py-16 text-center">
        <WifiOffIcon className="size-12 text-ink-3/50" />
        <h1 className="h-title">Device not detected</h1>
        <p className="h-sub max-w-md">
          Your device has not been detected on the network yet. Make sure you are accessing Wardnet
          directly from the local network. Connections through SSH tunnels or proxies cannot be
          matched to your device.
        </p>
      </div>
    );
  }

  return (
    <div className="col gap-20">
      <div className="col gap-2">
        <p className="text-xs text-ink-3">My device</p>
        <div className="flex items-center gap-3">
          <DeviceIcon type={device.device_type} size={28} className="text-ink/60" />
          <h1 className="h-title">{device.name ?? device.hostname ?? device.mac}</h1>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Internet access</CardTitle>
        </CardHeader>
        <CardContent>
          {adminLocked ? (
            <div className="col gap-3">
              <p className="text-sm">{routingLabel(currentRule, tunnels)}</p>
              <div className="flex items-start gap-2 text-ink-3">
                <LockIcon className="mt-0.5 size-4 shrink-0" />
                <p className="text-sm">
                  The network administrator is not allowing you to change your internet access
                  routing configuration.
                </p>
              </div>
            </div>
          ) : (
            <RoutingForm key={ruleKey} currentRule={currentRule} tunnels={tunnels} />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
