import { LaptopIcon } from "lucide-react";
import { Card, CardHeader, CardContent } from "@wardnet/forge-web/card";
import { StatTile } from "@wardnet/forge-web/stat-tile";
import type { Device } from "@wardnet/js";

const ONLINE_WINDOW_MS = 5 * 60_000;

function isOnline(device: Device): boolean {
  return Date.now() - new Date(device.last_seen).getTime() < ONLINE_WINDOW_MS;
}

interface Props {
  deviceCount: number;
  devices: Device[] | undefined;
  defaultPolicy: string | undefined;
}

export function DevicesCard({ deviceCount, devices, defaultPolicy }: Props) {
  const onlineCount = devices?.filter(isOnline).length ?? null;
  const defaultIsTunnel = defaultPolicy != null && defaultPolicy !== "direct";

  const tunnelledCount = devices?.filter((d) => {
    if (d.current_rule?.type === "tunnel") return true;
    if (d.current_rule === null && defaultIsTunnel) return true;
    return false;
  }).length ?? null;

  const sub =
    tunnelledCount !== null
      ? `${tunnelledCount} routed through a tunnel`
      : undefined;

  return (
    <Card>
      <CardHeader className="flex items-center gap-2 px-4 pt-4 pb-0 text-sm font-medium text-ink-2">
        <LaptopIcon size={14} strokeWidth={1.8} />
        Devices
      </CardHeader>
      <CardContent className="px-4 pb-4">
        <StatTile
          label="Online"
          value={onlineCount !== null ? `${onlineCount} / ${deviceCount}` : "—"}
          sub={sub}
        />
      </CardContent>
    </Card>
  );
}
