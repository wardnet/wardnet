import { useState } from "react";
import { Text } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { useDeviceTimeline } from "@wardnet/web";
import { StatusBadge } from "@/components/compound/StatusBadge";
import type {
  DeviceDhcpEvent,
  DeviceEventKind,
  DeviceTimelineEvent,
  DeviceTimelineWindow,
} from "@wardnet/js";

const WINDOWS: { value: DeviceTimelineWindow; label: string }[] = [
  { value: "one_hour", label: "1h" },
  { value: "six_hours", label: "6h" },
  { value: "twenty_four_hours", label: "24h" },
  { value: "seven_days", label: "7d" },
];

/** Plain-language label for an observation. */
function eventLabel(kind: DeviceEventKind): string {
  switch (kind) {
    case "discovered":
      return "Appeared";
    case "returned":
      return "Came back";
    case "gone":
      return "Went away";
    case "ip_changed":
      return "Changed address";
    case "zone_changed":
      return "Moved zone";
    case "routing_changed":
      return "Routing changed";
    case "conntrack_flushed":
      return "Connections reset";
    default:
      return kind;
  }
}

/**
 * A conntrack flush is the one entry that is something done *to* the device:
 * its live connections were torn down with nothing wrong on its own side.
 * Everything else here — a departure included — is ordinary for a phone that
 * sleeps, so colouring those would cry wolf.
 */
function eventTone(kind: DeviceEventKind): "neutral" | "danger" {
  return kind === "conntrack_flushed" ? "danger" : "neutral";
}

function dhcpLabel(event: DeviceDhcpEvent): string {
  switch (event.event_type) {
    case "assigned":
      return "Address assigned";
    case "renewed":
      return "Lease renewed";
    case "released":
      return "Lease released";
    case "expired":
      return "Lease expired";
    case "conflict":
      return "Address conflict";
    default:
      return event.event_type;
  }
}

function time(at: string): string {
  return new Date(at).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Compact rendering of an event's payload, or nothing when there is none. */
function detailText(event: DeviceTimelineEvent): string | null {
  const d = event.details;
  if (!d) return null;
  if (typeof d.old_ip === "string" && typeof d.new_ip === "string") {
    return `${d.old_ip} → ${d.new_ip}`;
  }
  if (typeof d.reason === "string") return d.reason;
  if (typeof d.ip === "string") return d.ip;
  if (typeof d.last_ip === "string") return d.last_ip;
  return null;
}

/**
 * The device's connectivity timeline: what it *did*, as opposed to the eight
 * cards above it, which are all configuration.
 *
 * The DNS row is split by result rather than shown as a total, because that
 * split is what settles the question the card exists to answer. A device at one
 * query a minute where every query succeeded has working DNS, and its problem
 * is somewhere else — a total cannot say that.
 */
export function DeviceTimelineCard({ deviceId }: { deviceId: string }) {
  const [window, setWindow] =
    useState<DeviceTimelineWindow>("twenty_four_hours");
  const { data, isLoading, isError } = useDeviceTimeline(deviceId, window);

  // A Map rather than an object: the keys are result slugs straight off the
  // wire, and indexing a plain object with them is an injection sink.
  const dnsTotals = new Map<string, number>();
  for (const bucket of data?.dns ?? []) {
    for (const [result, count] of Object.entries(bucket.results)) {
      dnsTotals.set(result, (dnsTotals.get(result) ?? 0) + count);
    }
  }
  const dnsResults = [...dnsTotals.entries()].sort((a, b) => b[1] - a[1]);

  const merged = [
    ...(data?.events ?? []).map((e) => ({
      at: e.at,
      label: eventLabel(e.kind),
      tone: eventTone(e.kind),
      detail: detailText(e),
    })),
    ...(data?.dhcp ?? []).map((e) => ({
      at: e.at,
      label: dhcpLabel(e),
      tone: "neutral" as const,
      detail: e.details,
    })),
  ].sort((a, b) => b.at.localeCompare(a.at));

  return (
    <Card>
      <CardHeader>
        <CardTitle>Activity</CardTitle>
        <CardAction>
          <div role="group" aria-label="Time window">
            {WINDOWS.map((w) => (
              <button
                key={w.value}
                type="button"
                aria-pressed={w.value === window}
                onClick={() => setWindow(w.value)}
              >
                {w.label}
              </button>
            ))}
          </div>
        </CardAction>
      </CardHeader>
      <CardContent>
        {isLoading && <Text>Loading activity…</Text>}
        {isError && <Text>Could not load this device's activity.</Text>}

        {data && (
          <>
            <div>
              <Text>DNS queries</Text>
              {dnsResults.length === 0 ? (
                <Text>
                  No DNS queries in this window. That is normal for a device
                  that has been idle, and expected if query logging is off.
                </Text>
              ) : (
                <ul>
                  {dnsResults.map(([result, count]) => (
                    <li key={result}>
                      <span>{result.replace(/_/g, " ")}</span>
                      <span>{count}</span>
                    </li>
                  ))}
                </ul>
              )}
            </div>

            <div>
              <Text>Events</Text>
              {merged.length === 0 ? (
                <Text>
                  Nothing recorded in this window — the device stayed put and
                  kept its address.
                </Text>
              ) : (
                <ul>
                  {merged.map((entry, i) => (
                    <li key={`${entry.at}-${i}`}>
                      <span>{time(entry.at)}</span>
                      <StatusBadge tone={entry.tone}>{entry.label}</StatusBadge>
                      {entry.detail && <span>{entry.detail}</span>}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
