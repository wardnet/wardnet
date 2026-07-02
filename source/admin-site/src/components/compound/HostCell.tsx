import type { Device } from "@wardnet/js";

/**
 * MAC-keyed lookup map for matching DHCP/lease records to known devices.
 * MAC casing is canonical (lowercase) on the server (issue #312), so no
 * per-key normalisation is needed.
 */
// eslint-disable-next-line react-refresh/only-export-components
export function buildDeviceIndex(devices: Device[]): Map<string, Device> {
  const map = new Map<string, Device>();
  for (const d of devices) map.set(d.mac, d);
  return map;
}

/**
 * Two-line "Host" cell shared by Devices, DHCP leases, and reservations:
 * an icon on the left, then a primary identifier with a smaller secondary
 * line beneath. Keeps tabular layouts visually consistent.
 *
 * `note` is an optional muted annotation rendered under the secondary line —
 * DHCP uses it to surface a reservation's description ("why this reservation
 * exists") directly in the row.
 *
 * `extra` is rendered under the note — useful for collapsing a sibling column
 * into this cell on narrow viewports (e.g. DHCP's IP column dropping into the
 * Host cell on mobile via `md:hidden`).
 */
export function HostCell({
  primary,
  secondary,
  note,
  extra,
  icon,
}: {
  primary: string;
  secondary: string | null;
  note?: string | null;
  extra?: React.ReactNode;
  icon: React.ReactNode;
}) {
  return (
    <div className="host">
      <div className="avatar">{icon}</div>
      <div className="col">
        <div className="name">{primary}</div>
        {secondary && <div className="mac mono">{secondary}</div>}
        {note && <div className="text-ink-3 text-xs">{note}</div>}
        {extra}
      </div>
    </div>
  );
}
