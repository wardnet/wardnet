import { memo, useCallback, useMemo, useState } from "react";
import { useDevices, useTunnels, useDefaultPolicy, countryFlag, isDeviceOnline } from "@wardnet/wardnet-web";
import { DeviceRoutingSheet } from "@/components/DeviceRoutingSheet";
import { ChevronRightIcon } from "lucide-react";
import type { Device, Tunnel } from "@wardnet/js";

type Filter = "all" | "online" | "vpn";

const FILTER_LABELS: Record<Filter, string> = { all: "All", online: "Online", vpn: "On VPN" };

function isOnVpn(device: Device, defaultPolicy: string | undefined): boolean {
  if (device.current_rule?.type === "tunnel") return true;
  if (
    (device.current_rule === null || device.current_rule.type === "default") &&
    defaultPolicy !== undefined &&
    defaultPolicy !== "direct"
  ) return true;
  return false;
}

function routeLabel(device: Device, tunnels: Tunnel[]): string {
  const rule = device.current_rule;
  if (!rule || rule.type === "default") return "Default";
  if (rule.type === "direct") return "Direct";
  const tunnel = tunnels.find((t) => t.id === rule.tunnel_id);
  return tunnel ? `${countryFlag(tunnel.country_code)} ${tunnel.label}` : "Via tunnel";
}

type Annotated = { device: Device; online: boolean; onVpn: boolean };

function sortAnnotated(annotated: Annotated[]): Annotated[] {
  return [...annotated].sort((a, b) => {
    if (a.online !== b.online) return a.online ? -1 : 1;
    const la = (a.device.name ?? a.device.hostname ?? a.device.mac).toLowerCase();
    const lb = (b.device.name ?? b.device.hostname ?? b.device.mac).toLowerCase();
    return la.localeCompare(lb);
  });
}

const DeviceRow = memo(function DeviceRow({
  device,
  online,
  tunnels,
  onSelect,
}: {
  device: Device;
  online: boolean;
  tunnels: Tunnel[];
  onSelect: (id: string) => void;
}) {
  return (
    <button
      onClick={() => onSelect(device.id)}
      className="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors duration-snap active:bg-sunken first:rounded-t-xl last:rounded-b-xl"
    >
      <span className={[
        "mt-0.5 size-2 shrink-0 self-start rounded-full",
        online ? "bg-accent" : "bg-line-strong",
      ].join(" ")} />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-[14px] font-medium text-ink">
          {device.name ?? device.hostname ?? device.mac}
        </span>
        <span className="truncate text-[12px] text-ink-3">
          {device.last_ip} · {routeLabel(device, tunnels)}
        </span>
      </div>
      <ChevronRightIcon size={16} className="shrink-0 text-ink-4" />
    </button>
  );
});

export default function Devices() {
  const { data: devicesData, isLoading } = useDevices();
  const { data: tunnelsData } = useTunnels();
  const { data: policyData } = useDefaultPolicy();

  const [filter, setFilter] = useState<Filter>("all");
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);
  const [sheetOpen, setSheetOpen] = useState(false);

  const allDevices = devicesData?.devices ?? [];
  const tunnels = tunnelsData?.tunnels ?? [];
  const defaultPolicy = policyData?.policy;

  // Derived from live query data — always reflects the latest refetch.
  const selectedDevice = useMemo(
    () => allDevices.find((d) => d.id === selectedDeviceId) ?? null,
    [allDevices, selectedDeviceId],
  );

  // Single pass: annotate each device once; counts and visible both derive from this.
  const annotated = useMemo<Annotated[]>(
    () => allDevices.map((d) => ({
      device: d,
      online: isDeviceOnline(d.last_seen),
      onVpn: isOnVpn(d, defaultPolicy),
    })),
    [allDevices, defaultPolicy],
  );

  const counts = useMemo(() => ({
    all: annotated.length,
    online: annotated.filter((a) => a.online).length,
    vpn: annotated.filter((a) => a.onVpn).length,
  }), [annotated]);

  const visible = useMemo(
    () => sortAnnotated(
      annotated.filter((a) =>
        filter === "online" ? a.online :
        filter === "vpn"    ? a.onVpn : true
      )
    ),
    [annotated, filter],
  );

  const handleDeviceClick = useCallback((id: string) => {
    setSelectedDeviceId(id);
    setSheetOpen(true);
  }, []);

  if (isLoading) {
    return (
      <div className="flex flex-col gap-0 p-4">
        <h1 className="mb-4 text-xl font-semibold text-ink">Devices</h1>
        <div className="mb-3 flex gap-2">
          {[80, 100, 90].map((w, i) => (
            <div key={i} className="h-8 animate-pulse rounded-full bg-sunken" style={{ width: w }} />
          ))}
        </div>
        {Array.from({ length: 6 }).map((_, i) => (
          <div key={i} className="flex items-center gap-3 border-b border-line py-3">
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
      <h1 className="mb-4 text-xl font-semibold text-ink">Devices</h1>

      {/* Filter pills */}
      <div className="mb-3 flex gap-2 overflow-x-auto pb-1">
        {(["all", "online", "vpn"] as Filter[]).map((id) => (
          <button
            key={id}
            onClick={() => setFilter(id)}
            className={[
              "shrink-0 rounded-full px-3.5 py-1.5 text-[13px] font-medium transition-colors duration-snap",
              filter === id ? "bg-accent text-accent-ink" : "bg-sunken text-ink-3 active:bg-line",
            ].join(" ")}
          >
            {FILTER_LABELS[id]} ({counts[id]})
          </button>
        ))}
      </div>

      {/* Device list */}
      {visible.length === 0
        ? <p className="py-16 text-center text-sm text-ink-3">No devices match this filter.</p>
        : (
          <div className="flex flex-col divide-y divide-line rounded-xl border border-line bg-card">
            {visible.map(({ device, online }) => (
              <DeviceRow
                key={device.id}
                device={device}
                online={online}
                tunnels={tunnels}
                onSelect={handleDeviceClick}
              />
            ))}
          </div>
        )}

      <DeviceRoutingSheet
        device={selectedDevice}
        tunnels={tunnels}
        open={sheetOpen}
        onOpenChange={setSheetOpen}
      />
    </div>
  );
}
