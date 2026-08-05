import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { manufacturerDisplay, timeAgo } from "@wardnet/web";
import type { Device } from "@wardnet/js";

interface DeviceIdentityCardProps {
  device: Device;
}

/** Read-only identity card: MAC, hostname, manufacturer, first/last seen. */
export function DeviceIdentityCard({ device }: DeviceIdentityCardProps) {
  const manufacturer = manufacturerDisplay(device);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Identity</CardTitle>
      </CardHeader>
      <CardContent>
        <Text
          as="dl"
          size="sm"
          className="grid grid-cols-2 gap-x-6 gap-y-4 md:grid-cols-3"
        >
          <div>
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              MAC
            </Text>
            <dd>
              <span className="font-mono">{device.mac}</span>
              {device.is_randomized && (
                <Text
                  as="span"
                  size="xs"
                  className="ml-2 rounded bg-surface-2 px-1.5 py-0.5 text-ink-3"
                  title="This device presents a randomized (private) MAC address to avoid being tracked across networks. It is not the address burned in by its manufacturer."
                >
                  Randomized
                </Text>
              )}
            </dd>
          </div>
          <div>
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              Hostname
            </Text>
            <dd>{device.hostname ?? "-"}</dd>
          </div>
          <div>
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              Manufacturer
            </Text>
            <dd
              className={manufacturer.unknown ? "text-ink-3" : undefined}
              title={manufacturer.hint ?? undefined}
            >
              {manufacturer.label}
            </dd>
          </div>
          <div>
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              First seen
            </Text>
            <dd>{timeAgo(device.first_seen)}</dd>
          </div>
          <div>
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              Last seen
            </Text>
            <dd>{timeAgo(device.last_seen)}</dd>
          </div>
        </Text>
      </CardContent>
    </Card>
  );
}
