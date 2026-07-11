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

interface InboundWgDevicePickerProps {
  /** Candidate devices — callers pre-filter to those without an existing
   *  remote-access credential (`connection_mode !== "remote"`). */
  devices: Device[];
  /** Currently selected device id. Empty string = none chosen yet. */
  value: string;
  onChange: (deviceId: string) => void;
  id?: string;
}

/**
 * Device picker for granting inbound-WireGuard remote access, id-keyed
 * (`AddInboundWgPeerRequest.device_id`) — distinct from
 * `DeviceSelect`, which is IP-keyed for routing/filter contexts.
 */
export function InboundWgDevicePicker({
  devices,
  value,
  onChange,
  id,
}: InboundWgDevicePickerProps) {
  const selected = devices.find((d) => d.id === value);
  const triggerLabel = selected ? (
    <span className="flex min-w-0 items-center gap-2">
      <DeviceIcon type={selected.device_type} size={16} />
      <Text as="span" weight="medium" className="truncate">
        {selected.name || selected.hostname || selected.last_ip}
      </Text>
    </span>
  ) : (
    <span className="text-ink-3">Choose a device</span>
  );

  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger id={id} className="w-full">
        <SelectValue asChild placeholder="Choose a device">
          {triggerLabel}
        </SelectValue>
      </SelectTrigger>
      <SelectContent>
        {devices.length === 0 && (
          <div className="px-2 py-1.5">
            <Text as="span" size="sm" className="text-ink-3">
              Every known device already has remote access.
            </Text>
          </div>
        )}
        {devices.map((d) => {
          const primary = d.name || d.hostname || d.last_ip;
          const secondary = d.name || d.hostname ? d.last_ip : null;
          return (
            <SelectItem key={d.id} value={d.id}>
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
