import type { DeviceSignal, DeviceSignalKind } from "@wardnet/js";

/**
 * How one kind of identification signal should be presented (issue #1099).
 *
 * Each kind needs its own explanation because they are not equally meaningful:
 * a DHCP hostname is whatever the device asked to be called, while an answering
 * port is something we went and checked. Rendering them as an undifferentiated
 * list would flatten that difference.
 */
export interface DeviceSignalKindDisplay {
  /** Group heading. */
  label: string;
  /** One-line explanation of where this kind of observation comes from. */
  hint: string;
}

/**
 * A Map rather than an object literal: the key arrives from the daemon, so an
 * unrecognised kind has to miss cleanly rather than index into an object.
 */
const SIGNAL_KIND_DISPLAY = new Map<DeviceSignalKind, DeviceSignalKindDisplay>([
  [
    "dhcp_hostname",
    {
      label: "Hostname (DHCP)",
      hint: "The name the device asked to be known by when it requested an address.",
    },
  ],
  [
    "dhcp_vendor_class",
    {
      label: "Vendor class (DHCP)",
      hint: "A vendor string the device sent with its address request. Many IoT stacks put a literal brand name here.",
    },
  ],
  [
    "dhcp_param_list",
    {
      label: "DHCP fingerprint",
      hint: "The list of options the device asked for, in order. The ordering is characteristic of a device class rather than of one manufacturer.",
    },
  ],
  [
    "mdns_service",
    {
      label: "Announced services (mDNS)",
      hint: "Services the device advertised on the local network, such as casting or smart-home control.",
    },
  ],
  [
    "probed_port",
    {
      label: "Answering ports",
      hint: "Ports that responded when you asked Wardnet to identify this device.",
    },
  ],
]);

/**
 * Display metadata for a signal kind. Falls back to the raw kind so a signal
 * written by a newer daemon still renders instead of disappearing.
 */
export function signalKindDisplay(
  kind: DeviceSignalKind,
): DeviceSignalKindDisplay {
  return (
    SIGNAL_KIND_DISPLAY.get(kind) ?? {
      label: kind,
      hint: "An identification signal recorded by a newer version of Wardnet.",
    }
  );
}

/** Signals of one kind, in the order they should be rendered. */
export interface DeviceSignalGroup {
  kind: DeviceSignalKind;
  display: DeviceSignalKindDisplay;
  signals: DeviceSignal[];
}

/**
 * The order signal kinds are shown in: most directly useful for naming a
 * device first, with the fingerprint (which identifies a class, not a vendor)
 * last.
 */
const KIND_ORDER: DeviceSignalKind[] = [
  "mdns_service",
  "dhcp_vendor_class",
  "dhcp_hostname",
  "probed_port",
  "dhcp_param_list",
];

/**
 * Group signals by kind for display, dropping empty groups and preserving the
 * server's most-recent-first order within each one.
 */
export function groupSignalsByKind(
  signals: DeviceSignal[],
): DeviceSignalGroup[] {
  const seen = new Map<DeviceSignalKind, DeviceSignal[]>();
  for (const signal of signals) {
    const bucket = seen.get(signal.kind);
    if (bucket) bucket.push(signal);
    else seen.set(signal.kind, [signal]);
  }

  // Known kinds in their intended order, then anything a newer daemon wrote,
  // in first-seen order — an unrecognised kind is still evidence.
  const ordered: DeviceSignalKind[] = [
    ...KIND_ORDER.filter((k) => seen.has(k)),
    ...[...seen.keys()].filter((k) => !KIND_ORDER.includes(k)),
  ];

  return ordered.map((kind) => ({
    kind,
    display: signalKindDisplay(kind),
    signals: seen.get(kind) ?? [],
  }));
}
