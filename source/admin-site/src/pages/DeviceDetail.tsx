import { Link, useParams } from "react-router";
import { DetailPageHeader } from "@/components/compound/DetailPageHeader";
import { DeviceIcon } from "@wardnet/web";
import { Heading, Text } from "@wardnet/web";
import { StatusBadge } from "@/components/compound/StatusBadge";
import { DeviceDnsFilterCard } from "@/components/features/DeviceDnsFilterCard";
import { DeviceDnsCaptureCard } from "@/components/features/DeviceDnsCaptureCard";
import { DeviceIdentityCard } from "@/components/features/DeviceIdentityCard";
import { DeviceNetworkCard } from "@/components/features/DeviceNetworkCard";
import { DeviceSettingsCard } from "@/components/features/DeviceSettingsCard";
import { useDevice } from "@wardnet/web";
import { deviceTypeLabel } from "@wardnet/web";
import { timeAgo } from "@wardnet/web";
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
      <div className="col gap-20">
        <Text as="p" className="text-ink-3">
          Loading…
        </Text>
      </div>
    );
  }

  if (isError || !data) {
    return (
      <div className="col gap-8">
        <Heading level={1} size="3xl" className="text-ink">
          Device not found
        </Heading>
        <Text as="p" size="sm" className="mt-1 text-ink-3">
          The device you're looking for may have been removed.
        </Text>
        <Link to="/devices" className="text-accent">
          Back to Devices
        </Link>
      </div>
    );
  }

  const device = data.device;
  const managed = device.name != null;
  const online = managed && isOnline(device.last_seen);

  const status = managed ? (
    <StatusBadge tone={online ? "success" : "neutral"}>
      {online ? "Online" : "Offline"}
    </StatusBadge>
  ) : (
    <StatusBadge tone="neutral">Discovered</StatusBadge>
  );

  const typeLabel = deviceTypeLabel(device.device_type);

  return (
    <div className="col gap-20">
      <DetailPageHeader
        parentLabel="Devices"
        parentTo="/devices"
        itemLabel={deviceLabel(device)}
        icon={<DeviceIcon type={device.device_type} size={24} />}
        status={status}
        meta={
          <span>
            Last seen: {timeAgo(device.last_seen)} · {typeLabel} ·{" "}
            {device.last_ip}
          </span>
        }
      />

      <DeviceIdentityCard device={device} />
      <DeviceSettingsCard device={device} currentRule={data.current_rule} />
      <DeviceDnsFilterCard device={device} />
      <DeviceDnsCaptureCard deviceId={device.id} />
      <DeviceNetworkCard device={device} />
    </div>
  );
}
