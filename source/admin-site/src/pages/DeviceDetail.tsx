import { Link, useParams } from "react-router";
import { DetailPageHeader } from "@/components/compound/DetailPageHeader";
import { DeviceIcon } from "@wardnet/web";
import { Heading, Text } from "@wardnet/web";
import { StatusBadge } from "@/components/compound/StatusBadge";
import { DeviceDnsFilterCard } from "@/components/features/DeviceDnsFilterCard";
import { DeviceDnsCaptureCard } from "@/components/features/DeviceDnsCaptureCard";
import { DeviceIdentityCard } from "@/components/features/DeviceIdentityCard";
import { DeviceIdentificationCard } from "@/components/features/DeviceIdentificationCard";
import { DeviceNetworkCard } from "@/components/features/DeviceNetworkCard";
import { DeviceReleaseCard } from "@/components/features/DeviceReleaseCard";
import { DeviceSettingsCard } from "@/components/features/DeviceSettingsCard";
import { DeviceRoutingProfilesCard } from "@/components/features/DeviceRoutingProfilesCard";
import { DeviceZoneCard } from "@/components/features/DeviceZoneCard";
import {
  useDevice,
  useTunnels,
  useUpdateDevice,
  useReleaseDevice,
  useRoutingProfiles,
  useDeviceRoutingProfiles,
  useSetDeviceRoutingProfiles,
  useNetworkZones,
  useAssignDeviceZone,
  useDeviceFilterSettings,
  useDnsFilterProfiles,
  useDnsFilterConfig,
  useUpdateDeviceFilterSettings,
  useDnsCaptureSettings,
  useUpdateDnsCaptureSettings,
  useDhcpReservations,
  useCreateReservation,
  useDeleteReservation,
} from "@wardnet/web";
import { deviceTypeLabel } from "@wardnet/web";
import { timeAgo } from "@wardnet/web";
import type { Device, DeviceSignal, RoutingTarget } from "@wardnet/js";

const ONLINE_THRESHOLD_MS = 5 * 60 * 1000;

function isOnline(lastSeen: string): boolean {
  const ts = new Date(lastSeen).getTime();
  if (!Number.isFinite(ts)) return false;
  return Date.now() - ts <= ONLINE_THRESHOLD_MS;
}

function deviceLabel(device: Device): string {
  if (device.name) return device.name;
  if (device.hostname) return device.hostname;
  // Only an IEEE registrant is solid enough to name the page after. A curated
  // catalog guess or a signal-derived name would otherwise be stated as fact
  // in the title and heading — "Govee device" — which is exactly the hedging
  // rule the rest of the UI applies via `manufacturerDisplay` (issue #1099).
  if (device.manufacturer && device.manufacturer_source === "ieee") {
    return `${device.manufacturer} device`;
  }
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

  return (
    <DeviceDetailLoaded
      device={data.device}
      // `?? []` guards a browser holding a cached bundle newer than the
      // daemon it is talking to: a missing field must render the empty
      // state, not throw out the whole detail page.
      signals={data.signals ?? []}
      currentRule={data.current_rule}
    />
  );
}

interface DeviceDetailLoadedProps {
  device: Device;
  signals: DeviceSignal[];
  currentRule: RoutingTarget | null;
}

/** The loaded detail view. Split from the route component so the card
 *  queries/mutations mount only once the device itself has resolved — this
 *  is still the pages layer, and it owns every hook the cards need, passing
 *  data + callbacks down so the cards stay pure presentation. */
function DeviceDetailLoaded({
  device,
  signals,
  currentRule,
}: DeviceDetailLoadedProps) {
  // Settings card.
  const { data: tunnelData } = useTunnels();
  const updateDevice = useUpdateDevice();
  const releaseDevice = useReleaseDevice();

  // Routing profiles card.
  const { data: profilesData } = useRoutingProfiles();
  const { data: assignedData } = useDeviceRoutingProfiles(device.id);
  const saveRoutingProfiles = useSetDeviceRoutingProfiles();

  // Zone card.
  const { data: zoneData } = useNetworkZones();
  const assignZone = useAssignDeviceZone({ successMessage: "Zone updated" });

  // DNS filter card.
  const { data: filterSettingsData } = useDeviceFilterSettings(device.id);
  const { data: filterProfilesData } = useDnsFilterProfiles();
  const { data: filterConfigData } = useDnsFilterConfig();
  const updateFilterSettings = useUpdateDeviceFilterSettings();

  // DNS capture card.
  const { data: captureSettings, isLoading: captureLoading } =
    useDnsCaptureSettings(device.id);
  const updateCaptureSettings = useUpdateDnsCaptureSettings(device.id);

  // Network card.
  const { data: reservationsData } = useDhcpReservations();
  const createReservation = useCreateReservation();
  const restoreReservation = useCreateReservation({ silent: true });
  const deleteReservation = useDeleteReservation();

  // Server-owned flag (#1181), not inferred from the name — see Devices.tsx.
  const managed = device.managed;
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
      <DeviceIdentificationCard signals={signals} />
      <DeviceSettingsCard
        device={device}
        currentRule={currentRule}
        tunnels={tunnelData?.tunnels ?? []}
        updateDevice={updateDevice}
      />
      <DeviceRoutingProfilesCard
        device={device}
        allProfiles={profilesData?.profiles ?? []}
        assignedIds={assignedData?.profile_ids ?? []}
        save={saveRoutingProfiles}
      />
      <DeviceZoneCard
        device={device}
        zones={zoneData?.zones ?? []}
        assignZone={assignZone}
      />
      <DeviceDnsFilterCard
        device={device}
        settings={filterSettingsData?.settings}
        profiles={filterProfilesData?.profiles}
        config={filterConfigData?.config}
        update={updateFilterSettings}
      />
      <DeviceDnsCaptureCard
        settings={captureSettings}
        isLoading={captureLoading}
        update={updateCaptureSettings}
      />
      <DeviceNetworkCard
        device={device}
        reservations={reservationsData?.reservations}
        createReservation={createReservation}
        restoreReservation={restoreReservation}
        deleteReservation={deleteReservation}
      />
      <DeviceReleaseCard device={device} release={releaseDevice} />
    </div>
  );
}
