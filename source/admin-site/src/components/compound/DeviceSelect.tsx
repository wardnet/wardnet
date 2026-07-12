import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import { DeviceIcon } from "@wardnet/web";
import { Text } from "@wardnet/web";
import type { Device } from "@wardnet/js";

interface DeviceSelectProps {
  /** Devices to choose from. Order is preserved. */
  devices: Device[];
  /** Currently selected value. Empty string = nothing selected (shows the
   *  sentinel/placeholder). */
  value: string;
  /** Called with the chosen value (or "" for the any-device sentinel). */
  onChange: (value: string) => void;
  /**
   * Which device field the option value is keyed by. `"ip"` (default) uses
   * `last_ip` — what the DNS query log indexes by, for filter pickers;
   * `"id"` uses the device id, for grant/assignment pickers that target a
   * specific device.
   */
  valueKey?: "ip" | "id";
  /** Label for the trigger when nothing is selected, and for the sentinel
   *  option when `includeAny` is true. */
  anyLabel?: string;
  /** When true (default), renders a sentinel "any" option (value `""`) so the
   *  select can represent "no filter". Set false for a required single pick. */
  includeAny?: boolean;
  /** Message rendered inside the dropdown when `devices` is empty and there is
   *  no sentinel option to fall back on. */
  emptyLabel?: string;
  /** Override the input id (for `<label htmlFor>`). */
  id?: string;
  /** Extra classes to apply to the underlying SelectTrigger — useful
   *  for sizing variants (`.select-trigger--md` / `--sm`) or width. */
  triggerClassName?: string;
}

/**
 * Device-aware select. Each option mirrors the HostCell layout used in
 * data tables — device-type icon on the left, name (or hostname / IP)
 * as the primary line, and IP as the secondary line when a friendlier
 * label exists. Keeps filter pickers visually consistent with the
 * tables they narrow.
 *
 * Two modes via `valueKey`/`includeAny`: an IP-keyed filter with an "any"
 * sentinel (the default), or an id-keyed required picker (e.g. selecting a
 * device to grant remote access to).
 */
export function DeviceSelect({
  devices,
  value,
  onChange,
  valueKey = "ip",
  anyLabel = "Any device",
  includeAny = true,
  emptyLabel,
  id,
  triggerClassName,
}: DeviceSelectProps) {
  const keyOf = (d: Device) => (valueKey === "id" ? d.id : d.last_ip);
  const selected = devices.find((d) => keyOf(d) === value);
  // Trigger label collapses to a single line (icon + name) so it fits
  // the narrow filter column. Dropdown items remain two-line so users
  // can disambiguate by IP.
  const triggerLabel = selected ? (
    <span className="flex min-w-0 items-center gap-2">
      <DeviceIcon type={selected.device_type} size={16} />
      <Text as="span" weight="medium" className="truncate">
        {selected.name || selected.hostname || selected.last_ip}
      </Text>
    </span>
  ) : (
    <span className="text-ink-3">{anyLabel}</span>
  );

  // Radix Select forbids an empty-string item value, so the "any" sentinel
  // maps to the reserved "any" token; without a sentinel, the empty value
  // simply shows the placeholder.
  const selectValue = includeAny ? value || "any" : value;
  const handleChange = (v: string) =>
    onChange(includeAny && v === "any" ? "" : v);

  return (
    <Select value={selectValue} onValueChange={handleChange}>
      {/* `w-full` overrides the SelectTrigger default of `w-fit` so the
          trigger fills its parent column instead of shrinking to the
          width of the longest device label. */}
      <SelectTrigger id={id} className={triggerClassName ?? "w-full"}>
        {/* `asChild` lets us bypass the SelectValue's default rendering
            (which mirrors the chosen SelectItem's children, IP and all)
            and supply our own compact label instead. */}
        <SelectValue asChild placeholder={anyLabel}>
          {triggerLabel}
        </SelectValue>
      </SelectTrigger>
      <SelectContent>
        {includeAny && (
          <SelectItem value="any">
            <span className="text-ink-3">{anyLabel}</span>
          </SelectItem>
        )}
        {!includeAny && devices.length === 0 && emptyLabel && (
          <div className="px-2 py-1.5">
            <Text as="span" size="sm" className="text-ink-3">
              {emptyLabel}
            </Text>
          </div>
        )}
        {devices.map((d) => {
          const primary = d.name || d.hostname || d.last_ip;
          const secondary = d.name || d.hostname ? d.last_ip : null;
          return (
            <SelectItem key={d.id} value={keyOf(d)}>
              <div className="flex items-center gap-2">
                <DeviceIcon type={d.device_type} size={16} />
                <div className="flex flex-col">
                  <Text as="span" weight="medium">
                    {primary}
                  </Text>
                  {secondary && (
                    <Text as="span" size="xs" className="text-ink-3">
                      {secondary}
                    </Text>
                  )}
                </div>
              </div>
            </SelectItem>
          );
        })}
      </SelectContent>
    </Select>
  );
}
