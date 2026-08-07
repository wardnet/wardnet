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
          //
          // The copy names only what the daemon actually records today: the
          // DHCP options captured in `dhcp/server.rs`. Promising mDNS or port
          // observation here would tell an admin their device announces
          // nothing when the truth is that Wardnet never looked (#1115/#1116).
          <Text as="p" size="sm" className="text-ink-3">
            Nothing observed yet. Wardnet learns a device's identity from what
            it says when it asks for a network address, so a device with a fixed
            address — or one that has not renewed since Wardnet was installed —
            has never had the chance to tell us anything.
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
                      {/* Signal values run to 128 chars and DHCP option lists
                          carry no natural break opportunity, so the value has
                          to be allowed to shrink (`min-w-0`) and wrap
                          mid-token (`break-all`) or it pushes the card open
                          and scrolls the page sideways. */}
                      <Text
                        as="span"
                        size="sm"
                        className="min-w-0 break-all font-mono"
                      >
                        {signal.value}
                      </Text>
                      {signal.inferred && (
                        <Text
                          as="span"
                          size="xs"
                          className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 text-ink-3"
                          // Deliberately does NOT claim this signal named the
                          // device. Naming is first-writer-wins against an
                          // empty manufacturer, so a device already named by
                          // its IEEE registrant collects matching signals that
                          // changed nothing — and a device can match two
                          // different vendors at once (a TV answering both
                          // _googlecast._tcp and _airplay._tcp).
                          title="This value matches a vendor in Wardnet's own list. It is evidence about the device, not necessarily where its manufacturer name came from."
                        >
                          Matches vendor list
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
