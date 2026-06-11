import { WifiOffIcon } from "lucide-react";
import { useMyDevice, DeviceIcon } from "@wardnet/web";

/**
 * Home tab — device identity + (eventually) self-service routing control.
 *
 * The shell wires `useMyDevice` (the unauthenticated, device-keyed endpoint) and
 * renders the device-not-detected fallback when the caller's device can't be
 * matched on the LAN. The routing controls themselves land in #590; for now a
 * detected device shows only its identity header.
 */
export default function Home() {
  const { data, isLoading } = useMyDevice();
  const device = data?.device;

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
    <div className="flex flex-col gap-5 p-5">
      <div className="flex items-center gap-3">
        <DeviceIcon type={device.device_type} size={28} className="text-ink/60" />
        <h1 className="text-lg font-semibold text-ink">
          {device.name ?? device.hostname ?? device.mac}
        </h1>
      </div>
    </div>
  );
}
