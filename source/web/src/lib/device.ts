import type { Device, DeviceType } from "@wardnet/js";

const ONLINE_THRESHOLD_MS = 5 * 60 * 1000;

/** Returns true when the device was seen within the last 5 minutes. */
export function isDeviceOnline(lastSeen: string): boolean {
  const ts = new Date(lastSeen).getTime();
  return Number.isFinite(ts) && Date.now() - ts <= ONLINE_THRESHOLD_MS;
}

/** Display labels for each {@link DeviceType}, ordered for selection menus. */
export const DEVICE_TYPE_OPTIONS: { value: DeviceType; label: string }[] = [
  { value: "tv", label: "TV" },
  { value: "phone", label: "Phone" },
  { value: "laptop", label: "Laptop" },
  { value: "tablet", label: "Tablet" },
  { value: "game_console", label: "Console" },
  { value: "settop_box", label: "Set-top box" },
  { value: "iot", label: "IoT" },
  { value: "router", label: "Router" },
  { value: "managed_switch", label: "Managed switch" },
  { value: "server", label: "Server" },
  { value: "unknown", label: "Unknown" },
];

/** Human-readable label for a {@link DeviceType}; falls back to the raw value. */
export function deviceTypeLabel(type: DeviceType): string {
  return DEVICE_TYPE_OPTIONS.find((o) => o.value === type)?.label ?? type;
}

/** How a device's manufacturer should be presented, with the reason why. */
export interface ManufacturerDisplay {
  /** Text to render. */
  label: string;
  /** Explanation for a tooltip; `null` when the name needs no qualification. */
  hint: string | null;
  /** True when we have no name at all, so the UI can style it as absent. */
  unknown: boolean;
}

/**
 * Decide how to present a device's manufacturer (issue #1099).
 *
 * The point is to never let a blank read as a Wardnet failure when it is
 * actually the vendor's choice, and never to state a guess as fact:
 *
 * - `ieee` — the registrant on record; shown plainly.
 * - `catalog` — our curated mapping for an OUI the IEEE lists as `Private`;
 *   shown as "likely <vendor>" because the vendor deliberately did not publish
 *   this and the block could be reassigned.
 * - `signal` — inferred from what the device announced or answered.
 * - absent — genuinely unknown; the hint explains the two reasons why.
 */
export function manufacturerDisplay(device: Device): ManufacturerDisplay {
  if (!device.manufacturer) {
    return {
      label: "Unknown manufacturer",
      hint: device.is_randomized
        ? "This device uses a randomized (private) MAC address, which carries no manufacturer information."
        : "No manufacturer is registered for this address block, or the vendor chose a private IEEE listing that hides their name.",
      unknown: true,
    };
  }

  switch (device.manufacturer_source) {
    case "catalog":
      return {
        label: `Likely ${device.manufacturer}`,
        hint: "Identified from Wardnet's own vendor list. The manufacturer chose a private IEEE listing, so this is an informed guess rather than a registered fact.",
        unknown: false,
      };
    case "signal":
      return {
        label: device.manufacturer,
        hint: "Identified from what this device announced on the network rather than from its address block.",
        unknown: false,
      };
    default:
      return { label: device.manufacturer, hint: null, unknown: false };
  }
}
