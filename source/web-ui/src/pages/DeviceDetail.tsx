import { Link, useParams } from "react-router";
import { DetailPageHeader } from "@/components/compound/DetailPageHeader";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { StatusBadge } from "@/components/compound/StatusBadge";
import { DeviceIdentityCard } from "@/components/features/DeviceIdentityCard";
import { DeviceNetworkCard } from "@/components/features/DeviceNetworkCard";
import { DeviceSettingsCard } from "@/components/features/DeviceSettingsCard";
import { useDevice } from "@/hooks/useDevices";
import { deviceTypeLabel } from "@/lib/device";
import { timeAgo } from "@/lib/utils";
import type { Device } from "@wardnet/js";

const ONLINE_THRESHOLD_MS = 5 * 60 * 1000;

function isOnline(lastSeen: string): boolean {
  const ts = new Date(lastSeen).getTime();
  if (!Number.isFinite(ts)) return false;
  return Date.now() - ts <= ONLINE_THRESHOLD_MS;
}

function deviceLabel(device: Device): string {
  if (device.name) return device.name;
  if (device.hostname) return device.hostname;
  if (device.manufacturer) return `${device.manufacturer} device`;
  return device.mac;
}

/** Routed detail page for a single device. */
export default function DeviceDetail() {
  const { id = "" } = useParams<{ id: string }>();
  const { data, isLoading, isError } = useDevice(id);

  if (isLoading) {
    return (
      <div className="p-6">
        <p className="text-sm text-muted-foreground">Loading…</p>
      </div>
    );
  }

  if (isError || !data) {
    return (
      <div className="flex flex-col gap-2 p-6">
        <h1 className="text-xl font-semibold">Device not found</h1>
        <p className="text-sm text-muted-foreground">
          The device you're looking for may have been removed.
        </p>
        <Link to="/devices" className="text-sm text-primary underline">
          Back to Devices
        </Link>
      </div>
    );
  }

  const device = data.device;
  const managed = device.name != null;
  const online = managed && isOnline(device.last_seen);

  const status = managed ? (
    <StatusBadge tone={online ? "success" : "neutral"}>{online ? "Online" : "Offline"}</StatusBadge>
  ) : (
    <StatusBadge tone="neutral">Discovered</StatusBadge>
  );

  const typeLabel = deviceTypeLabel(device.device_type);

  return (
    <div className="flex flex-col gap-6 p-4 md:p-6">
      <DetailPageHeader
        parentLabel="Devices"
        parentTo="/devices"
        itemLabel={deviceLabel(device)}
        icon={<DeviceIcon type={device.device_type} size={24} />}
        status={status}
        meta={
          <span>
            Last seen: {timeAgo(device.last_seen)} · {typeLabel} · {device.last_ip}
          </span>
        }
      />

      <DeviceIdentityCard device={device} />
      <DeviceSettingsCard device={device} currentRule={data.current_rule} />
      <DeviceNetworkCard device={device} />
    </div>
  );
}
