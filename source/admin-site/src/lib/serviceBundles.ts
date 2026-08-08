import type { PortSpec, ServiceSpec } from "@wardnet/js";

/**
 * Curated cross-zone service options for the exceptions UI.
 *
 * The daemon models a service as either a built-in **preset** or an explicit
 * **port list**. `casting` and `smart_home` are real backend presets; every
 * other entry here is a UI-side convenience that expands to an explicit port
 * list. `custom` lets the operator enter arbitrary ports.
 *
 * Note the `web` bundle below is the intended companion to `smart_home` for
 * vendors whose local control is plain HTTP (Shelly, Tasmota, ESPHome's web
 * server): the preset deliberately omits 80/443 because, applied zone-to-zone,
 * they would reach every HTTP listener in the peer zone.
 */
export interface ServiceOption {
  id: string;
  label: string;
  /** The concrete spec, or `"custom"` for the manual port editor. */
  spec: ServiceSpec | "custom";
}

const port = (proto: "tcp" | "udp", from: number, to = from): PortSpec => ({
  proto,
  from,
  to,
});

export const SERVICE_OPTIONS: ServiceOption[] = [
  {
    id: "casting",
    label: "Casting (mDNS, Cast, AirPlay, DLNA)",
    spec: { type: "preset", set: "casting" },
  },
  {
    id: "smart-home",
    label: "Smart home (Govee, Tuya, Shelly, ESPHome, LIFX)",
    spec: { type: "preset", set: "smart_home" },
  },
  {
    id: "printing",
    label: "Printing (IPP, AirPrint)",
    spec: {
      type: "ports",
      ports: [port("tcp", 631), port("udp", 5353), port("tcp", 9100)],
    },
  },
  {
    id: "file-sharing",
    label: "File sharing (SMB)",
    spec: { type: "ports", ports: [port("tcp", 445), port("tcp", 139)] },
  },
  {
    id: "remote-desktop",
    label: "Remote desktop (RDP, VNC)",
    spec: { type: "ports", ports: [port("tcp", 3389), port("tcp", 5900)] },
  },
  {
    id: "ssh",
    label: "SSH",
    spec: { type: "ports", ports: [port("tcp", 22)] },
  },
  {
    id: "web",
    label: "Web (HTTP, HTTPS)",
    spec: { type: "ports", ports: [port("tcp", 80), port("tcp", 443)] },
  },
  { id: "custom", label: "Custom ports…", spec: "custom" },
];

/** Stable key for a port list, order-insensitive, for equality comparison. */
function portsKey(ports: PortSpec[]): string {
  return ports
    .map((p) => `${p.proto}:${p.from}-${p.to}`)
    .sort()
    .join(",");
}

/**
 * The friendly label for a stored {@link ServiceSpec}, matching it back to a
 * known bundle. Returns `null` for an unrecognized custom port list so callers
 * can fall back to a generic summary.
 */
export function matchServiceLabel(service: ServiceSpec): string | null {
  if (service.type === "preset") {
    const opt = SERVICE_OPTIONS.find(
      (o) =>
        o.spec !== "custom" &&
        o.spec.type === "preset" &&
        o.spec.set === service.set,
    );
    return opt?.label ?? null;
  }
  const key = portsKey(service.ports);
  const opt = SERVICE_OPTIONS.find(
    (o) =>
      o.spec !== "custom" &&
      o.spec.type === "ports" &&
      portsKey(o.spec.ports) === key,
  );
  return opt?.label ?? null;
}
