import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { groupSignalsByKind, timeAgo } from "@wardnet/web";
import type { DeviceSignal } from "@wardnet/js";

interface DeviceIdentificationCardProps {
  signals: DeviceSignal[];
}

/**
 * Read-only view of everything Wardnet has observed about a device's identity
 * (issue #1099).
 *
 * This is the provenance behind the manufacturer shown on the identity card:
 * when that reads "Likely Govee", the evidence for it is here.
 */
export function DeviceIdentificationCard({
  signals,
}: DeviceIdentificationCardProps) {
  const groups = groupSignalsByKind(signals);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Identification signals</CardTitle>
      </CardHeader>
      <CardContent>
        {groups.length === 0 ? (
          // An empty list is the ordinary case for a device that has only ever
          // been seen by ARP. Say so plainly — reading this as a Wardnet
          // failure is the exact confusion issue #1099 was filed about.
          <Text as="p" size="sm" className="text-ink-3">
            Nothing observed yet. Wardnet records identification signals as a
            device uses the network — when it requests an address, or announces
            a service. A device that only responds to address lookups gives
            nothing away.
          </Text>
        ) : (
          <div className="col gap-6">
            {groups.map((group) => (
              <div key={group.kind}>
                <Text
                  as="h3"
                  size="xs"
                  className="uppercase tracking-wide text-ink-3"
                  title={group.display.hint}
                >
                  {group.display.label}
                </Text>
                <ul className="mt-2 col gap-1">
                  {group.signals.map((signal) => (
                    <li
                      key={`${signal.kind}:${signal.value}`}
                      className="flex flex-wrap items-baseline gap-x-3"
                    >
                      <Text as="span" size="sm" className="font-mono">
                        {signal.value}
                      </Text>
                      {signal.inferred && (
                        <Text
                          as="span"
                          size="xs"
                          className="rounded bg-surface-2 px-1.5 py-0.5 text-ink-3"
                          title="This observation matched Wardnet's vendor list and is what named the device."
                        >
                          Matched vendor list
                        </Text>
                      )}
                      <Text as="span" size="xs" className="text-ink-3">
                        {timeAgo(signal.observed_at)}
                      </Text>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
