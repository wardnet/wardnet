import type { Device } from "@wardnet/js";

/**
 * MAC-keyed lookup map for matching DHCP/lease records to known devices.
 * Lower-cases on insert because MAC casing currently varies across the
 * stack (see wardnet/wardnet#312).
 */
export function buildDeviceIndex(devices: Device[]): Map<string, Device> {
  const map = new Map<string, Device>();
  for (const d of devices) map.set(d.mac.toLowerCase(), d);
  return map;
}

/**
 * Two-line "Host" cell shared by Devices, DHCP leases, and reservations:
 * an icon on the left, then a primary identifier with a smaller secondary
 * line beneath. Keeps tabular layouts visually consistent.
 *
 * `extra` is rendered under the secondary line — useful for collapsing a
 * sibling column into this cell on narrow viewports (e.g. DHCP's IP
 * column dropping into the Host cell on mobile via `md:hidden`).
 */
export function HostCell({
  primary,
  secondary,
  extra,
  icon,
}: {
  primary: string;
  secondary: string | null;
  extra?: React.ReactNode;
  icon: React.ReactNode;
}) {
  return (
    <div className="host">
      <div className="avatar">{icon}</div>
      <div className="col">
        <div className="name">{primary}</div>
        {secondary && <div className="mac mono">{secondary}</div>}
        {extra}
      </div>
    </div>
  );
}
