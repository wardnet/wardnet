import type { DeviceType } from "@wardnet/js";

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
