import { useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Button } from "@/components/core/ui/button";
import { RadioGroup, RadioGroupItem } from "@/components/core/ui/radio-group";
import { Label } from "@/components/core/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { useMyDevice, useSetMyRule } from "@/hooks/useDevices";
import { countryFlag } from "@/lib/country";
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

  // Determine initial mode and tunnel selection.
  const initialMode = currentRule?.type === "tunnel" && currentRule.tunnel_id ? "vpn" : "direct";
  const initialTunnelId =
    currentRule?.type === "tunnel" ? currentRule.tunnel_id : (tunnels[0]?.id ?? "");

  const [mode, setMode] = useState(initialMode);
  const [tunnelId, setTunnelId] = useState(initialTunnelId);

  const selectedTarget: RoutingTarget =
    mode === "vpn" && tunnelId ? { type: "tunnel", tunnel_id: tunnelId } : { type: "direct" };

  const hasChanges = !targetsEqual(selectedTarget, currentRule);

  async function handleSave() {
    await setMyRule.mutateAsync(selectedTarget);
  }

  return (
    <div className="flex flex-col gap-4">
      <RadioGroup value={mode} onValueChange={(v) => setMode(v as "direct" | "vpn")}>
        <div className="flex items-center gap-2">
          <RadioGroupItem value="direct" id="routing-direct" />
          <Label htmlFor="routing-direct">Direct (no VPN)</Label>
        </div>
        <div className="flex items-center gap-2">
          <RadioGroupItem value="vpn" id="routing-vpn" />
          <Label htmlFor="routing-vpn">VPN</Label>
        </div>
      </RadioGroup>

      {mode === "vpn" && tunnels.length > 0 && (
        <Select value={tunnelId} onValueChange={setTunnelId}>
          <SelectTrigger className="w-full">
            <SelectValue placeholder="Select a tunnel" />
          </SelectTrigger>
          <SelectContent>
            {tunnels.map((t) => {
              const flag = t.country_code ? countryFlag(t.country_code) : "";
              return (
                <SelectItem key={t.id} value={t.id}>
                  {flag && <span className="mr-1.5">{flag}</span>}
                  {t.label}
                </SelectItem>
              );
            })}
          </SelectContent>
        </Select>
      )}

      {mode === "vpn" && tunnels.length === 0 && (
        <p className="text-sm text-muted-foreground">
          No VPN tunnels are configured. Ask your network administrator to add one.
        </p>
      )}

      {setMyRule.isError && (
        <ApiErrorAlert error={setMyRule.error} fallback="Failed to update routing" />
      )}

      <Button onClick={handleSave} disabled={!hasChanges || setMyRule.isPending} className="w-full">
        {setMyRule.isPending ? "Saving..." : "Save"}
      </Button>
    </div>
  );
}

/** Self-service page showing the caller's device info and routing controls. */
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
    return (
      <div className="mx-auto max-w-lg pt-8">
        <p className="text-sm text-muted-foreground">Loading...</p>
      </div>
    );
  }

  if (!device) {
    return (
      <div className="mx-auto flex max-w-lg flex-col items-center gap-4 pt-16 text-center">
        <WifiOffIcon className="size-12 text-muted-foreground/50" />
        <h2 className="text-lg font-semibold">Device not detected</h2>
        <p className="text-sm text-muted-foreground">
          Your device has not been detected on the network yet. Make sure you are accessing Wardnet
          directly from the local network. Connections through SSH tunnels or proxies cannot be
          matched to your device.
        </p>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-lg pt-8">
      <p className="text-xs text-muted-foreground">My Device</p>
      <div className="mt-1 flex items-center gap-3">
        <DeviceIcon type={device.device_type} size={28} className="text-foreground/60" />
        <h1 className="text-2xl font-bold">{device.name ?? device.hostname ?? device.mac}</h1>
      </div>

      <Card className="mt-6">
        <CardHeader>
          <CardTitle className="text-sm font-medium">Internet Access</CardTitle>
        </CardHeader>
        <CardContent>
          {adminLocked ? (
            <div className="flex flex-col gap-3">
              <p className="text-sm">{routingLabel(currentRule, tunnels)}</p>
              <div className="flex items-start gap-2 text-muted-foreground">
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
