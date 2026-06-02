import { MonitorIcon } from "lucide-react";
import { Link } from "react-router";
import { Card } from "@wardnet/forge-web/card";
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

  const tunnelledCount =
    devices?.filter((d) => {
      if (d.current_rule?.type === "tunnel") return true;
      if (d.current_rule === null && defaultIsTunnel) return true;
      return false;
    }).length ?? null;

  return (
    <Link to="/devices" className="block">
      <Card className="card--flush">
        <div className="flex items-center gap-3 px-2 py-3">
          <div
            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
            style={{ background: "var(--color-accent-soft)", color: "var(--color-accent)" }}
          >
            <MonitorIcon size={22} strokeWidth={1.8} />
          </div>

          <div className="flex-1 min-w-0">
            <div className="text-[10px] font-semibold uppercase tracking-wider text-ink-3">
              Devices Online
            </div>
            <div className="mt-0.5 flex items-baseline gap-1.5">
              <span className="text-2xl font-bold text-ink">
                {onlineCount !== null ? onlineCount : "—"}
              </span>
              <span className="text-sm text-ink-3">/ {deviceCount}</span>
            </div>
            {tunnelledCount !== null && (
              <div className="mt-0.5 text-xs text-ink-3">
                {tunnelledCount} routed through a tunnel
              </div>
            )}
          </div>
        </div>
      </Card>
    </Link>
  );
}
