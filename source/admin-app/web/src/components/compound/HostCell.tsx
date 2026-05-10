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
 */
export function HostCell({
  primary,
  secondary,
  icon,
}: {
  primary: string;
  secondary: string | null;
  icon: React.ReactNode;
}) {
  return (
    <div className="host">
      <div className="avatar">{icon}</div>
      <div className="col">
        <div className="name">{primary}</div>
        {secondary && <div className="mac mono">{secondary}</div>}
      </div>
    </div>
  );
}
