import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Badge } from "@/components/core/ui/badge";
import { PageHeader } from "@/components/compound/PageHeader";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { useMyDevice } from "@/hooks/useDevices";
import type { RoutingTarget } from "@wardnet/js";

function ruleLabel(target: RoutingTarget | null) {
  if (!target) return "Default";
  switch (target.type) {
    case "direct":
      return "Direct";
    case "tunnel":
      return "Via tunnel";
    case "default":
      return "Default";
  }
}

/** Self-service page showing the caller's device info and routing status. */
export default function MyDevice() {
  const { data, isLoading } = useMyDevice();
  const device = data?.device;
  const rule = data?.current_rule ?? null;

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
            <div className="flex items-center gap-3">
              <Badge variant="secondary">{ruleLabel(rule)}</Badge>
              {data?.admin_locked && (
                <span className="text-xs text-muted-foreground">Locked by admin</span>
              )}
            </div>
            <p className="mt-3 text-xs text-muted-foreground">
              Your traffic routing is managed by Wardnet. Contact your network admin to change it.
            </p>
          </CardContent>
        </Card>
      </div>
    </>
  );
}
