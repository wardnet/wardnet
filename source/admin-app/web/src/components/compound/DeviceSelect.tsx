import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/forge-web/select";
import { DeviceIcon } from "./DeviceIcon";
import type { Device } from "@wardnet/js";

interface DeviceSelectProps {
  /** Devices to choose from. Order is preserved. */
  devices: Device[];
  /** Currently selected device IP. Empty string = "any". */
  value: string;
  /** Called with the chosen IP (or "" for the any-device sentinel). */
  onChange: (ip: string) => void;
  /** Override the placeholder / "any" option label. */
  anyLabel?: string;
  /** Override the input id (for `<label htmlFor>`). */
  id?: string;
}

/**
 * Device-aware select. Each option mirrors the HostCell layout used in
 * data tables — device-type icon on the left, name (or hostname / IP)
 * as the primary line, and IP as the secondary line when a friendlier
 * label exists. Keeps filter pickers visually consistent with the
 * tables they narrow.
 *
 * The select's value is the device's `last_ip` because that's what the
 * DNS query log indexes by; "" is the sentinel for "any device".
 */
export function DeviceSelect({
  devices,
  value,
  onChange,
  anyLabel = "Any device",
  id,
}: DeviceSelectProps) {
  const selected = devices.find((d) => d.last_ip === value);
  // Trigger label collapses to a single line (icon + name) so it fits
  // the narrow filter column. Dropdown items remain two-line so users
  // can disambiguate by IP.
  const triggerLabel = selected ? (
    <span className="flex min-w-0 items-center gap-2">
      <DeviceIcon type={selected.device_type} size={16} />
      <span className="truncate font-medium">
        {selected.name || selected.hostname || selected.last_ip}
      </span>
    </span>
  ) : (
    <span className="text-ink-3">{anyLabel}</span>
  );

  return (
    <Select value={value || "any"} onValueChange={(v) => onChange(v === "any" ? "" : v)}>
      {/* `w-full` overrides the SelectTrigger default of `w-fit` so the
          trigger fills its parent column instead of shrinking to the
          width of the longest device label. */}
      <SelectTrigger id={id} className="w-full">
        {/* `asChild` lets us bypass the SelectValue's default rendering
            (which mirrors the chosen SelectItem's children, IP and all)
            and supply our own compact label instead. */}
        <SelectValue asChild placeholder={anyLabel}>
          {triggerLabel}
        </SelectValue>
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="any">
          <span className="text-ink-3">{anyLabel}</span>
        </SelectItem>
        {devices.map((d) => {
          const primary = d.name || d.hostname || d.last_ip;
          const secondary = d.name || d.hostname ? d.last_ip : null;
          return (
            <SelectItem key={d.id} value={d.last_ip}>
              <div className="flex items-center gap-2">
                <DeviceIcon type={d.device_type} size={16} />
                <div className="flex flex-col">
                  <span className="font-medium">{primary}</span>
                  {secondary && <span className="text-xs text-ink-3">{secondary}</span>}
                </div>
              </div>
            </SelectItem>
          );
        })}
      </SelectContent>
    </Select>
  );
}
