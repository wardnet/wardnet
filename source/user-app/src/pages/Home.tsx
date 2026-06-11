import { useState } from "react";
import {
  LockIcon,
  RefreshCwIcon,
  ShieldCheckIcon,
  ShieldXIcon,
  WifiOffIcon,
} from "lucide-react";
import { MapContainer, Marker, TileLayer } from "react-leaflet";
import { useQueryClient } from "@tanstack/react-query";
import {
  ApiErrorAlert,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  DeviceIcon,
  Pill,
  RoutingSelector,
  TunnelStatusPill,
  countryFlag,
  useMyDevice,
  useSetMyRule,
} from "@wardnet/web";
import type { RoutingTarget, TunnelSummary } from "@wardnet/js";
import { useIpGeolocation } from "@/hooks/useIpGeolocation";

function targetsEqual(
  a: RoutingTarget | null,
  b: RoutingTarget | null,
): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  if (a.type !== b.type) return false;
  switch (a.type) {
    case "tunnel":
      return b.type === "tunnel" && a.tunnel_id === b.tunnel_id;
    case "direct":
    case "default":
      return true;
  }
}

function routingLabel(
  target: RoutingTarget | null,
  tunnels: TunnelSummary[],
): string {
  if (!target || target.type === "default" || target.type === "direct") {
    return "Direct (no VPN)";
  }
  if (target.type === "tunnel") {
    const t = tunnels.find((tun) => tun.id === target.tunnel_id);
    if (t) {
      const flag = t.country_code ? countryFlag(t.country_code) : "";
      return `VPN: ${flag} ${t.label}`.trim();
    }
    return "VPN";
  }
  return "Direct (no VPN)";
}

function RoutingForm({
  currentRule,
  tunnels,
  onRouteChanged,
}: {
  currentRule: RoutingTarget | null;
  tunnels: TunnelSummary[];
  onRouteChanged: () => void;
}) {
  const setMyRule = useSetMyRule();

  const [target, setTarget] = useState<RoutingTarget>(
    currentRule?.type === "tunnel" ? currentRule : { type: "direct" },
  );

  const hasChanges = !targetsEqual(target, currentRule);

  async function handleSave() {
    await setMyRule.mutateAsync(target);
    onRouteChanged();
  }

  return (
    <div className="flex flex-col gap-4">
      <RoutingSelector value={target} onChange={setTarget} tunnels={tunnels} />

      {setMyRule.isError && (
        <ApiErrorAlert
          error={setMyRule.error}
          fallback="Failed to update routing"
        />
      )}

      <Button
        onClick={handleSave}
        disabled={!hasChanges || setMyRule.isPending}
        className="w-full"
      >
        {setMyRule.isPending ? "Saving…" : "Save"}
      </Button>
    </div>
  );
}

function GeoDataRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-2 py-1.5 text-sm">
      <span className="text-ink-3">{label}</span>
      <span className="text-right font-medium text-ink">{value}</span>
    </div>
  );
}

function VerifyCard({
  activeTunnel,
}: {
  activeTunnel: TunnelSummary | null;
}) {
  const { data: geo, isLoading, isError, refetch, isFetching } =
    useIpGeolocation();

  const matchState =
    geo && activeTunnel
      ? geo.country_code === activeTunnel.country_code
        ? "match"
        : "mismatch"
      : "neutral";

  return (
    <Card>
      <CardHeader>
        <CardTitle>Verify your route</CardTitle>
        {activeTunnel && matchState !== "neutral" && (
          <span className="ml-auto">
            {matchState === "match" ? (
              <Pill variant="ok" className="flex items-center gap-1">
                <ShieldCheckIcon className="size-3" />
                Match
              </Pill>
            ) : (
              <Pill variant="warn" className="flex items-center gap-1">
                <ShieldXIcon className="size-3" />
                Mismatch
              </Pill>
            )}
          </span>
        )}
      </CardHeader>
      <CardContent className="p-0">
        {isLoading ? (
          <div className="flex h-[180px] items-center justify-center text-sm text-ink-3">
            Checking…
          </div>
        ) : isError ? (
          <div className="flex flex-col items-center gap-3 px-5 py-8 text-center">
            <ShieldXIcon className="size-8 text-ink-3/50" />
            <p className="text-sm text-ink-3">
              Could not reach the geolocation service. Check your connection and
              try again.
            </p>
            <Button variant="outline" size="sm" onClick={() => refetch()}>
              Retry
            </Button>
          </div>
        ) : geo ? (
          <>
            <MapContainer
              center={[geo.latitude, geo.longitude]}
              zoom={8}
              zoomControl={false}
              dragging={false}
              scrollWheelZoom={false}
              style={{ height: "180px", width: "100%" }}
            >
              <TileLayer
                url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
                attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors'
              />
              <Marker position={[geo.latitude, geo.longitude]} />
            </MapContainer>
            <div className="divide-y divide-white/[0.06] px-5">
              <GeoDataRow label="IP" value={geo.ip} />
              <GeoDataRow
                label="Country"
                value={`${geo.country_code ? countryFlag(geo.country_code) : ""} ${geo.country_name}`.trim()}
              />
              <GeoDataRow label="City" value={geo.city} />
              <GeoDataRow label="ISP" value={geo.org} />
            </div>
            <div className="flex justify-end px-5 pb-4 pt-3">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => refetch()}
                disabled={isFetching}
                className="flex items-center gap-1.5 text-xs text-ink-3"
              >
                <RefreshCwIcon
                  className={`size-3 ${isFetching ? "animate-spin" : ""}`}
                />
                Refresh
              </Button>
            </div>
          </>
        ) : null}
      </CardContent>
    </Card>
  );
}

/**
 * Home tab — device identity + self-service routing control + route verification.
 *
 * `useMyDevice` is the unauthenticated, device-keyed endpoint (there is no
 * login in the User PWA — identity is the device on the LAN). When the caller
 * can't be matched to a device we show the not-detected fallback. Otherwise the
 * device picks its own internet routing, unless an admin has locked the rule —
 * in which case the current target is shown read-only with a lock and an
 * explanation.
 *
 * The verify card auto-fetches the device's public IP via ipapi.co (client-side
 * so the request egresses through the Pi's per-device routing) and re-checks
 * automatically one second after the user saves a new routing target.
 */
export default function Home() {
  const queryClient = useQueryClient();
  const { data, isLoading } = useMyDevice();

  const device = data?.device;
  const currentRule = data?.current_rule ?? null;
  const adminLocked = data?.admin_locked ?? false;
  const tunnels = data?.available_tunnels ?? [];
  const activeTunnel =
    currentRule?.type === "tunnel"
      ? (tunnels.find((t) => t.id === currentRule.tunnel_id) ?? null)
      : null;

  const ruleKey =
    currentRule?.type === "tunnel"
      ? `tunnel-${currentRule.tunnel_id}`
      : String(currentRule?.type ?? "null");

  function handleRouteChanged() {
    setTimeout(
      () => queryClient.invalidateQueries({ queryKey: ["ip-geolocation"] }),
      1_000,
    );
  }

  if (isLoading) {
    return <p className="p-5 text-sm text-ink-3">Loading…</p>;
  }

  if (!device) {
    return (
      <div className="flex flex-col items-center gap-4 px-5 py-16 text-center">
        <WifiOffIcon className="size-12 text-ink-3/50" />
        <h1 className="text-lg font-semibold text-ink">Device not detected</h1>
        <p className="max-w-md text-sm text-ink-3">
          Your device has not been detected on the network yet. Make sure you are
          accessing Wardnet directly from the local network. Connections through
          SSH tunnels or proxies cannot be matched to your device.
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6 p-5">
      <div className="flex items-center gap-3">
        <DeviceIcon
          type={device.device_type}
          size={28}
          className="text-ink/60"
        />
        <h1 className="text-lg font-semibold text-ink">
          {device.name ?? device.hostname ?? device.mac}
        </h1>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Internet route</CardTitle>
          {activeTunnel && (
            <span className="ml-auto">
              <TunnelStatusPill status={activeTunnel.status} />
            </span>
          )}
        </CardHeader>
        <CardContent>
          {adminLocked ? (
            <div className="flex flex-col gap-3">
              <p className="text-sm text-ink">
                {routingLabel(currentRule, tunnels)}
              </p>
              <div className="flex items-start gap-2 text-ink-3">
                <LockIcon className="mt-0.5 size-4 shrink-0" />
                <p className="text-sm">
                  Your network administrator has locked this setting, so you
                  can't change how your device connects to the internet.
                </p>
              </div>
            </div>
          ) : (
            <RoutingForm
              key={ruleKey}
              currentRule={currentRule}
              tunnels={tunnels}
              onRouteChanged={handleRouteChanged}
            />
          )}
        </CardContent>
      </Card>

      <VerifyCard activeTunnel={activeTunnel} />
    </div>
  );
}
