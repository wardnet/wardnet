import { MonitorIcon } from "lucide-react";
import { Link } from "react-router";
import { Card, Text } from "@wardnet/web";
import { isDeviceOnline } from "@wardnet/web";
import type { Device } from "@wardnet/js";

interface Props {
  deviceCount: number;
  devices: Device[] | undefined;
  defaultPolicy: string | undefined;
}

export function DevicesCard({ deviceCount, devices, defaultPolicy }: Props) {
  const onlineCount =
    devices?.filter((d) => isDeviceOnline(d.last_seen)).length ?? null;
  const defaultIsTunnel = defaultPolicy != null && defaultPolicy !== "direct";

  const tunnelledCount =
    devices?.filter((d) => {
      if (d.current_rule?.type === "tunnel") return true;
      if (
        (d.current_rule === null || d.current_rule.type === "default") &&
        defaultIsTunnel
      )
        return true;
      return false;
    }).length ?? null;

  return (
    <Link to="/devices" className="block" data-testid="dashboard-devices-card">
      <Card className="card--flush">
        <div className="flex items-center gap-3 px-2 py-3">
          <div
            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
            style={{
              background: "var(--color-accent-soft)",
              color: "var(--color-accent)",
            }}
          >
            <MonitorIcon size={22} strokeWidth={1.8} />
          </div>

          <div className="flex-1 min-w-0">
            <Text
              as="div"
              size="2xs"
              weight="semibold"
              className="uppercase tracking-wider text-ink-3"
            >
              Devices Online
            </Text>
            <div className="mt-0.5 flex items-baseline gap-1.5">
              <Text as="span" size="2xl" weight="bold" className="text-ink">
                {onlineCount !== null ? onlineCount : "-"}
              </Text>
              <Text as="span" size="sm" className="text-ink-3">
                / {deviceCount}
              </Text>
            </div>
            {tunnelledCount !== null && (
              <Text as="div" size="xs" className="mt-0.5 text-ink-3">
                {tunnelledCount} routed through a tunnel
              </Text>
            )}
          </div>
        </div>
      </Card>
    </Link>
  );
}
