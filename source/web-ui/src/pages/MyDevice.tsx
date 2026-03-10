import { useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Button } from "@/components/core/ui/button";
import { PageHeader } from "@/components/compound/PageHeader";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { RoutingSelector } from "@/components/compound/RoutingSelector";
import { useMyDevice, useSetMyRule } from "@/hooks/useDevices";
import { useTunnels } from "@/hooks/useTunnels";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import type { RoutingTarget, Tunnel } from "@wardnet/js";
import { LockIcon } from "lucide-react";

function targetsEqual(a: RoutingTarget | null, b: RoutingTarget | null): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  if (a.type !== b.type) return false;
  if (a.type === "tunnel" && b.type === "tunnel") return a.tunnel_id === b.tunnel_id;
  return true;
}

function RoutingForm({
  currentRule,
  tunnels,
}: {
  currentRule: RoutingTarget | null;
  tunnels: Tunnel[];
}) {
  const setMyRule = useSetMyRule();
  const [selectedTarget, setSelectedTarget] = useState<RoutingTarget | null>(currentRule);
  const hasChanges = !targetsEqual(selectedTarget, currentRule);

  async function handleSave() {
    if (!selectedTarget) return;
    await setMyRule.mutateAsync(selectedTarget);
  }

  return (
    <div className="flex flex-col gap-4">
      <RoutingSelector value={selectedTarget} onChange={setSelectedTarget} tunnels={tunnels} />
      {setMyRule.isError && (
        <ApiErrorAlert error={setMyRule.error} fallback="Failed to update routing" />
      )}
      <Button onClick={handleSave} disabled={!hasChanges || setMyRule.isPending} className="w-full">
        {setMyRule.isPending ? "Saving..." : "Save"}
      </Button>
    </div>
  );
}

/** Self-service page showing the caller's device info and routing status. */
export default function MyDevice() {
  const { data, isLoading } = useMyDevice();
  const { data: tunnelData } = useTunnels();

  const device = data?.device;
  const currentRule = data?.current_rule ?? null;
  const adminLocked = data?.admin_locked ?? false;
  const tunnels = tunnelData?.tunnels ?? [];

  const ruleKey =
    currentRule?.type === "tunnel"
      ? `tunnel-${currentRule.tunnel_id}`
      : String(currentRule?.type ?? "null");

  return (
    <>
      <PageHeader title="My Device" />
      <div className="grid gap-4 sm:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium text-muted-foreground">Device Info</CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <p className="text-sm text-muted-foreground">Loading...</p>
            ) : device ? (
              <div className="flex flex-col gap-4">
                <div className="flex items-center gap-3">
                  <DeviceIcon type={device.device_type} size={24} className="text-foreground/60" />
                  <p className="text-lg font-bold">
                    {device.name ?? device.hostname ?? device.mac}
                  </p>
                </div>
                <div className="grid grid-cols-2 gap-4 sm:grid-cols-3">
                  <div>
                    <p className="text-xs text-muted-foreground">IP</p>
                    <p className="font-mono text-sm">{device.last_ip}</p>
                  </div>
                  <div>
                    <p className="text-xs text-muted-foreground">MAC</p>
                    <p className="font-mono text-sm">{device.mac}</p>
                  </div>
                  {device.manufacturer && (
                    <div>
                      <p className="text-xs text-muted-foreground">Manufacturer</p>
                      <p className="text-sm">{device.manufacturer}</p>
                    </div>
                  )}
                </div>
              </div>
            ) : (
              <div className="flex flex-col gap-2">
                <p className="text-sm text-muted-foreground">
                  Your device has not been detected yet.
                </p>
                <p className="text-xs text-muted-foreground/70">
                  Make sure you are accessing Wardnet directly from the local network. Connections
                  through SSH tunnels or proxies cannot be matched to your device.
                </p>
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium text-muted-foreground">Routing</CardTitle>
          </CardHeader>
          <CardContent>
            {!device ? (
              <p className="text-sm text-muted-foreground">
                Routing options will appear once your device is detected.
              </p>
            ) : adminLocked ? (
              <div className="flex flex-col gap-3">
                <div className="flex items-center gap-2 text-muted-foreground">
                  <LockIcon className="size-4" />
                  <span className="text-sm">Locked by admin</span>
                </div>
                <RoutingSelector
                  value={currentRule}
                  onChange={() => {}}
                  tunnels={tunnels}
                  disabled
                />
              </div>
            ) : (
              <RoutingForm key={ruleKey} currentRule={currentRule} tunnels={tunnels} />
            )}
          </CardContent>
        </Card>
      </div>
    </>
  );
}
