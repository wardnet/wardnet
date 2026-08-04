import { memo, useCallback, useMemo, useState } from "react";
import {
  useDevices,
  useTunnels,
  useDefaultPolicy,
  usePendingDevices,
  useUpdateDevice,
  useNetworkZones,
  useAssignDeviceZone,
  useInboundWgPeers,
  countryFlag,
  deviceDisplayName,
  isDeviceOnline,
  timeAgo,
  Text,
  Heading,
  Pill,
} from "@wardnet/web";
import { useOnlineStatusContext } from "@/context/OnlineStatusContext";
import { DeviceRoutingSheet } from "@/components/DeviceRoutingSheet";
import { InboundWgPeerSheet } from "@/components/InboundWgPeerSheet";
import { ChevronRightIcon } from "lucide-react";
import type {
  Device,
  InboundWgPeerSummary,
  RoutingTarget,
  Tunnel,
} from "@wardnet/js";

type Filter = "all" | "online" | "vpn";

const FILTER_LABELS: Record<Filter, string> = {
  all: "All",
  online: "Online",
  vpn: "On VPN",
};

function isOnVpn(device: Device, defaultPolicy: string | undefined): boolean {
  if (device.current_rule?.type === "tunnel") return true;
  if (
    (device.current_rule === null || device.current_rule.type === "default") &&
    defaultPolicy !== undefined &&
    defaultPolicy !== "direct"
  )
    return true;
  return false;
}

function routeLabel(device: Device, tunnels: Tunnel[]): string {
  const rule = device.current_rule;
  if (!rule || rule.type === "default") return "Default";
  if (rule.type === "direct") return "Direct";
  const tunnel = tunnels.find((t) => t.id === rule.tunnel_id);
  return tunnel
    ? `${countryFlag(tunnel.country_code)} ${tunnel.label}`
    : "Via tunnel";
}

type Annotated = { device: Device; online: boolean; onVpn: boolean };

function sortAnnotated(annotated: Annotated[]): Annotated[] {
  return [...annotated].sort((a, b) => {
    if (a.online !== b.online) return a.online ? -1 : 1;
    const la = (
      a.device.name ??
      a.device.hostname ??
      a.device.mac
    ).toLowerCase();
    const lb = (
      b.device.name ??
      b.device.hostname ??
      b.device.mac
    ).toLowerCase();
    return la.localeCompare(lb);
  });
}

const DeviceRow = memo(function DeviceRow({
  device,
  online,
  tunnels,
  remotePeer,
  onSelect,
  onSelectPeer,
}: {
  device: Device;
  online: boolean;
  tunnels: Tunnel[];
  remotePeer?: InboundWgPeerSummary;
  onSelect: (id: string) => void;
  onSelectPeer: (peer: InboundWgPeerSummary) => void;
}) {
  const displayName = device.name ?? device.hostname ?? device.mac;
  return (
    // A row container (not itself a button): the device area and the Remote
    // badge are separate sibling buttons. Nesting the badge inside the row
    // button was invalid (interactive-in-interactive) and left the badge
    // keyboard-inaccessible.
    <div className="flex w-full items-center gap-3 px-4 py-3 transition-colors duration-snap active:bg-sunken first:rounded-t-xl last:rounded-b-xl">
      <button
        data-testid="device-row"
        onClick={() => onSelect(device.id)}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
      >
        <span
          className={[
            "mt-0.5 size-2 shrink-0 self-start rounded-full",
            online ? "bg-accent" : "bg-line-strong",
          ].join(" ")}
        />
        <div className="flex min-w-0 flex-1 flex-col">
          <Text
            as="span"
            size="base"
            weight="medium"
            className="truncate text-ink"
          >
            {displayName}
          </Text>
          <Text as="span" size="xs" className="truncate text-ink-3">
            {device.last_ip} · {routeLabel(device, tunnels)}
          </Text>
        </div>
      </button>
      {remotePeer && (
        <button
          type="button"
          data-testid="device-remote-badge"
          aria-label={`Manage remote access for ${displayName}`}
          onClick={() => onSelectPeer(remotePeer)}
          className="shrink-0"
        >
          <Pill variant={remotePeer.enabled ? "info" : "ghost"}>Remote</Pill>
        </button>
      )}
      <ChevronRightIcon size={16} className="shrink-0 text-ink-4" />
    </div>
  );
});

/**
 * "New devices awaiting review" — the client-derived quarantine inbox. New
 * devices land in the default-for-new zone; this surfaces the most-recent ones
 * so an admin can approve (reassign to the home zone) or tap to pick another
 * zone. This is the deep-link target for the #764 new-device push.
 */
function NewDevicesSection({ onSelect }: { onSelect: (id: string) => void }) {
  const { pending, homeZone } = usePendingDevices();
  const approve = useAssignDeviceZone({ successMessage: "Device approved" });

  if (pending.length === 0) return null;

  return (
    <div className="mb-4" data-testid="new-devices-section">
      <Text
        as="p"
        size="xs"
        weight="medium"
        className="mb-1.5 uppercase tracking-wider text-ink-3"
      >
        New devices awaiting review ({pending.length})
      </Text>
      <div className="flex flex-col divide-y divide-line rounded-xl border border-line bg-card">
        {pending.map((device) => (
          <div
            key={device.id}
            className="flex items-center gap-3 px-4 py-3"
            data-testid="new-device"
          >
            <button
              onClick={() => onSelect(device.id)}
              className="flex min-w-0 flex-1 flex-col text-left"
              data-testid="new-device-row"
            >
              <Text
                as="span"
                size="base"
                weight="medium"
                className="truncate text-ink"
              >
                {deviceDisplayName(device)}
              </Text>
              <Text as="span" size="xs" className="truncate text-ink-3">
                Joined {timeAgo(device.first_seen)}
              </Text>
            </button>
            {homeZone && (
              <button
                data-testid="new-device-approve"
                disabled={approve.isPending}
                onClick={() =>
                  approve.mutate({ deviceId: device.id, zoneId: homeZone.id })
                }
                className="shrink-0 rounded-full bg-accent px-3.5 py-1.5 text-accent-ink transition-colors duration-snap active:opacity-80 disabled:pointer-events-none disabled:opacity-40"
              >
                <Text as="span" size="sm" weight="medium">
                  Approve
                </Text>
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

export default function Devices() {
  const { data: devicesData, isLoading: devicesLoading } = useDevices();
  const { data: tunnelsData } = useTunnels();
  const { data: policyData, isLoading: policyLoading } = useDefaultPolicy();
  const { data: peersData } = useInboundWgPeers();
  const { data: zoneData } = useNetworkZones();
  const updateDevice = useUpdateDevice({ successMessage: "Routing updated" });
  const assignZone = useAssignDeviceZone({ successMessage: "Zone updated" });
  const isLoading = devicesLoading || policyLoading;

  const { showingLastKnownState } = useOnlineStatusContext();

  const [filter, setFilter] = useState<Filter>("all");
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);
  const [sheetOpen, setSheetOpen] = useState(false);
  const [selectedPeer, setSelectedPeer] = useState<InboundWgPeerSummary | null>(
    null,
  );
  const [peerSheetOpen, setPeerSheetOpen] = useState(false);

  const allDevices = devicesData?.devices ?? [];
  const tunnels = tunnelsData?.tunnels ?? [];
  const defaultPolicy = policyData?.policy;
  const peers = peersData?.peers ?? [];
  const zones = zoneData?.zones ?? [];
  const routingBusy = updateDevice.isPending || assignZone.isPending;

  const peersByDevice = useMemo(() => {
    const map = new Map<string, InboundWgPeerSummary>();
    for (const p of peers) if (p.device_id) map.set(p.device_id, p);
    return map;
  }, [peers]);

  const handleSelectPeer = useCallback((peer: InboundWgPeerSummary) => {
    setSelectedPeer(peer);
    setPeerSheetOpen(true);
  }, []);

  // Derived from live query data — always reflects the latest refetch.
  const selectedDevice = useMemo(
    () => allDevices.find((d) => d.id === selectedDeviceId) ?? null,
    [allDevices, selectedDeviceId],
  );

  // Single pass: annotate each device once; counts and visible both derive from this.
  const annotated = useMemo<Annotated[]>(
    () =>
      allDevices.map((d) => ({
        device: d,
        online: isDeviceOnline(d.last_seen),
        onVpn: isOnVpn(d, defaultPolicy),
      })),
    [allDevices, defaultPolicy],
  );

  const counts = useMemo(
    () => ({
      all: annotated.length,
      online: annotated.filter((a) => a.online).length,
      vpn: annotated.filter((a) => a.onVpn).length,
    }),
    [annotated],
  );

  const visible = useMemo(
    () =>
      sortAnnotated(
        annotated.filter((a) =>
          filter === "online" ? a.online : filter === "vpn" ? a.onVpn : true,
        ),
      ),
    [annotated, filter],
  );

  const handleDeviceClick = useCallback((id: string) => {
    setSelectedDeviceId(id);
    setSheetOpen(true);
  }, []);

  // Routing/zone mutations for the open sheet, hoisted out of the sheet so it
  // takes them as props (the sheet still owns its local remote-access grant
  // flow, which keeps its own one-time response state). Keyed on the selected
  // id rather than the derived `selectedDevice` so a background refetch that
  // drops the row still fires the mutation — matching the sheet's old latched
  // behavior — instead of silently no-oping. The page closes the sheet on
  // success.
  function handleSelectRoute(target: RoutingTarget) {
    if (!selectedDeviceId) return;
    updateDevice.mutate(
      { id: selectedDeviceId, body: { routing_target: target } },
      { onSuccess: () => setSheetOpen(false) },
    );
  }

  function handleSelectZone(zoneId: string) {
    if (!selectedDeviceId) return;
    assignZone.mutate(
      { deviceId: selectedDeviceId, zoneId },
      { onSuccess: () => setSheetOpen(false) },
    );
  }

  if (isLoading) {
    return (
      <div className="flex flex-col gap-0 p-4">
        <div className="mb-4">
          <Heading level={1} size="3xl" weight="bold" className="text-ink">
            Devices
          </Heading>
          <Text as="p" size="base" className="text-ink-3">
            Manage devices and routing overrides.
          </Text>
        </div>
        <div className="mb-3 flex gap-2">
          {[80, 100, 90].map((w, i) => (
            <div
              key={i}
              className="h-8 animate-pulse rounded-full bg-sunken"
              style={{ width: w }}
            />
          ))}
        </div>
        {Array.from({ length: 6 }).map((_, i) => (
          <div
            key={i}
            className="flex items-center gap-3 border-b border-line py-3"
          >
            <div className="size-9 animate-pulse rounded-lg bg-sunken" />
            <div className="flex flex-col gap-1.5">
              <div className="h-3.5 w-32 animate-pulse rounded bg-sunken" />
              <div className="h-3 w-24 animate-pulse rounded bg-sunken" />
            </div>
          </div>
        ))}
      </div>
    );
  }

  return (
    <div className="flex flex-col p-4">
      <div className="mb-4">
        <Heading level={1} size="3xl" weight="bold" className="text-ink">
          Devices
        </Heading>
        <Text as="p" size="base" className="text-ink-3">
          Manage devices and routing overrides.
        </Text>
      </div>

      <div
        className={
          showingLastKnownState
            ? "pointer-events-none opacity-40 transition-opacity"
            : "transition-opacity"
        }
      >
        <NewDevicesSection onSelect={handleDeviceClick} />

        {/* Filter pills */}
        <div className="mb-3 flex gap-2 overflow-x-auto pb-1">
          {(["all", "online", "vpn"] as Filter[]).map((id) => (
            <button
              key={id}
              data-testid={`device-filter-${id}`}
              onClick={() => setFilter(id)}
              className={[
                "shrink-0 rounded-full px-3.5 py-1.5 transition-colors duration-snap",
                filter === id
                  ? "bg-accent text-accent-ink"
                  : "bg-sunken text-ink-3 active:bg-line",
              ].join(" ")}
            >
              <Text as="span" size="sm" weight="medium">
                {/* eslint-disable-next-line security/detect-object-injection -- id from .map() over the hardcoded ["all","online","vpn"] Filter literal array */}
                {FILTER_LABELS[id]} ({counts[id]})
              </Text>
            </button>
          ))}
        </div>

        {/* Device list */}
        {visible.length === 0 ? (
          <Text as="p" size="sm" className="py-16 text-center text-ink-3">
            No devices match this filter.
          </Text>
        ) : (
          <div className="flex flex-col divide-y divide-line rounded-xl border border-line bg-card">
            {visible.map(({ device, online }) => (
              <DeviceRow
                key={device.id}
                device={device}
                online={online}
                tunnels={tunnels}
                remotePeer={peersByDevice.get(device.id)}
                onSelect={handleDeviceClick}
                onSelectPeer={handleSelectPeer}
              />
            ))}
          </div>
        )}
      </div>

      <DeviceRoutingSheet
        device={selectedDevice}
        tunnels={tunnels}
        zones={zones}
        busy={routingBusy}
        open={sheetOpen}
        onOpenChange={setSheetOpen}
        onSelectRoute={handleSelectRoute}
        onSelectZone={handleSelectZone}
      />

      <InboundWgPeerSheet
        peer={selectedPeer}
        open={peerSheetOpen}
        onOpenChange={setPeerSheetOpen}
      />
    </div>
  );
}
