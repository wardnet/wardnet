import { useState } from "react";
import {
  Button,
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Text } from "@wardnet/web";
import { groupSignalsByKind, timeAgo } from "@wardnet/web";
import type { MutationHandle } from "@/lib/mutationHandle";
import type { DeviceProbeResponse, DeviceSignal } from "@wardnet/js";

interface DeviceIdentificationCardProps {
  deviceId: string;
  signals: DeviceSignal[];
  /**
   * Whether the device is currently on the network. The daemon refuses a probe
   * for a departed device — its last known address may since have been handed
   * to someone else — so the action is disabled rather than left to fail.
   */
  online: boolean;
  /** The page's hoisted identification-probe mutation (issue #1116). */
  identify: MutationHandle<string, DeviceProbeResponse>;
}

/**
 * Everything Wardnet has observed about a device's identity (issue #1099),
 * plus the action that goes and asks for more (issue #1116).
 *
 * This is the provenance behind the manufacturer shown on the identity card:
 * when that reads "Likely Govee", the evidence for it is here. The probe action
 * lives on this card because this card is what it populates.
 */
export function DeviceIdentificationCard({
  deviceId,
  signals,
  online,
  identify,
}: DeviceIdentificationCardProps) {
  const groups = groupSignalsByKind(signals);
  // The probe outcome is deliberately not persisted (no `last_probed_at`
  // column — ADR 0025 §5), so an empty result exists only for as long as this
  // page stays open. Holding it here is what lets the card say "nothing
  // answered" instead of appearing not to have done anything.
  const [emptyProbe, setEmptyProbe] = useState<number | null>(null);

  async function runProbe() {
    setEmptyProbe(null);
    const result = await identify.mutateAsync(deviceId);
    setEmptyProbe(
      result.answering_ports.length === 0 ? result.ports_probed.length : null,
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Identification signals</CardTitle>
        <CardAction>
          <Button
            variant="outline"
            disabled={!online || identify.isPending}
            onClick={() => void runProbe()}
            title={
              online
                ? undefined
                : "This device is not currently on the network. Wardnet only probes the address it can still see, because that address may since have been given to a different device."
            }
          >
            {identify.isPending ? "Identifying…" : "Identify this device"}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        {/* Consent-shaped: the probe is the one identification mechanism that
            *sends* something, so say what pressing the button does before it
            is pressed rather than after (ADR 0025 §5). */}
        <Text as="p" size="sm" className="mb-6 text-ink-3">
          Identifying contacts this device directly on a short list of ports
          that smart-home vendors are known to use, and records whichever
          answer. Wardnet never does this on its own — only when you ask, and
          only for the device you are looking at.
        </Text>

        {emptyProbe !== null && (
          <Text as="p" size="sm" className="mb-6 text-ink-3">
            No known ports answered. Wardnet tried {emptyProbe} and none of them
            responded, which is normal for a device that is not a smart-home
            product — or one that simply does not answer strangers.
          </Text>
        )}
        {groups.length === 0 ? (
          // An empty list is the ordinary case for a device that has only ever
          // been seen by ARP. Say so plainly — reading this as a Wardnet
          // failure is the exact confusion issue #1099 was filed about.
          //
          // The copy names only what the daemon actually records today: the
          // DHCP options captured in `dhcp/server.rs`, plus whatever the probe
          // above finds. Promising passive mDNS observation here would tell an
          // admin their device announces nothing when the truth is that
          // Wardnet never looked (#1115).
          <Text as="p" size="sm" className="text-ink-3">
            Nothing observed yet. Wardnet learns a device's identity from what
            it says when it asks for a network address, so a device with a fixed
            address — or one that has not renewed since Wardnet was installed —
            has never had the chance to tell us anything. Identifying it above
            is the way to ask directly.
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
