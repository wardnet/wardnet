import { useState } from "react";
import { Drawer, DrawerContent, DrawerTitle, Text, Button } from "@wardnet/web";
import {
  useAddInboundWgPeer,
  useInboundWgConfig,
  useDdnsStatus,
  placeholderEndpoint,
  buildInboundWgClientConfig,
  InboundWgQrCode,
} from "@wardnet/web";
import type { AddInboundWgPeerResponse, Device } from "@wardnet/js";
import { CheckIcon } from "lucide-react";

interface Props {
  /** Candidate devices — pre-filtered to those without an existing
   *  remote-access credential (`connection_mode !== "remote"`). */
  devices: Device[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function DeviceOption({
  device,
  selected,
  onSelect,
}: {
  device: Device;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      onClick={onSelect}
      className="flex w-full items-center gap-3 px-4 py-3.5 text-left transition-colors duration-snap active:bg-sunken"
      data-testid="inbound-wg-grant-device-option"
    >
      {selected ? (
        <CheckIcon size={16} className="shrink-0 text-accent" />
      ) : (
        <span className="size-4 shrink-0" />
      )}
      <div className="flex min-w-0 flex-1 flex-col">
        <Text as="span" size="lg" weight="medium" className="text-ink">
          {device.name ?? device.hostname ?? device.mac}
        </Text>
        <Text as="span" size="xs" className="text-ink-3">
          {device.last_ip}
        </Text>
      </div>
    </button>
  );
}

/** Grant flow (issue #813): pick a device, then show the QR code / `.conf`
 *  download for its freshly-issued credential. The private key exists only
 *  in this response — shown once, never persisted, never re-fetchable. */
export function InboundWgGrantSheet({ devices, open, onOpenChange }: Props) {
  const addPeer = useAddInboundWgPeer();
  const { data: config } = useInboundWgConfig();
  const { data: ddns } = useDdnsStatus();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [granted, setGranted] = useState<AddInboundWgPeerResponse | null>(null);

  function reset() {
    setSelectedId(null);
    setGranted(null);
  }

  function handleClose(next: boolean) {
    if (!next) reset();
    onOpenChange(next);
  }

  async function handleGrant() {
    if (!selectedId) return;
    const response = await addPeer.mutateAsync({ device_id: selectedId });
    setGranted(response);
  }

  const endpoint = config
    ? placeholderEndpoint(
        config.listen_port,
        ddns?.fqdn ?? null,
        ddns?.last_public_ip ?? null,
      )
    : null;
  const clientConfig =
    granted && config?.server_public_key && endpoint
      ? buildInboundWgClientConfig({
          privateKey: granted.private_key,
          allowedIp: granted.allowed_ip,
          serverPublicKey: config.server_public_key,
          endpoint,
        })
      : null;

  return (
    <Drawer open={open} onOpenChange={handleClose}>
      <DrawerContent side="bottom" aria-describedby={undefined}>
        <div className="mx-auto mt-3 mb-4 h-1 w-10 rounded-full bg-line" />
        <DrawerTitle className="px-4 pb-1 text-[11px] font-semibold uppercase tracking-wider text-ink-3">
          {granted ? `Granted — ${granted.name}` : "Grant remote access"}
        </DrawerTitle>
        <div
          data-testid="inbound-wg-grant-sheet"
          className="flex flex-col"
          style={{ paddingBottom: "max(24px, env(safe-area-inset-bottom))" }}
        >
          {!granted ? (
            <>
              {devices.length === 0 ? (
                <Text as="p" size="sm" className="px-4 py-6 text-ink-3">
                  Every known device already has remote access.
                </Text>
              ) : (
                devices.map((d) => (
                  <DeviceOption
                    key={d.id}
                    device={d}
                    selected={selectedId === d.id}
                    onSelect={() => setSelectedId(d.id)}
                  />
                ))
              )}
              <div className="px-4 pt-3">
                <Button
                  className="w-full"
                  onClick={handleGrant}
                  disabled={!selectedId || addPeer.isPending}
                >
                  {addPeer.isPending ? "Granting…" : "Grant"}
                </Button>
              </div>
            </>
          ) : (
            <div className="flex flex-col items-center gap-4 px-4 pb-2">
              {clientConfig ? (
                <InboundWgQrCode value={clientConfig} size={220} />
              ) : (
                <Text as="p" size="sm" className="text-center text-danger">
                  No public hostname or IP is known yet — set up remote access
                  first to get a usable config.
                </Text>
              )}
              <Text as="p" size="xs" className="text-center text-ink-3">
                This is the only time the private key is shown. Endpoint is a
                placeholder — full relay wiring is tracked separately.
              </Text>
              <Button className="w-full" onClick={() => handleClose(false)}>
                I've saved this
              </Button>
            </div>
          )}
        </div>
      </DrawerContent>
    </Drawer>
  );
}
