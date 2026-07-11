import { useState } from "react";
import {
  Card,
  CardAction,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import {
  useInboundWgConfig,
  useSetInboundWgConfig,
  useInboundWgPeers,
  useAddInboundWgPeer,
  useRemoveInboundWgPeer,
  useSetInboundWgPeerEnabled,
  useDevices,
  useDdnsStatus,
  placeholderEndpoint,
} from "@wardnet/web";
import { PageHeader } from "@/components/compound/PageHeader";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import { InboundWgDevicePicker } from "@/components/compound/InboundWgDevicePicker";
import { InboundWgPeersTable } from "@/components/compound/InboundWgPeersTable";
import { InboundWgPeerGrantedModal } from "@/components/features/InboundWgPeerGrantedModal";
import type {
  AddInboundWgPeerResponse,
  InboundWgPeerSummary,
} from "@wardnet/js";

const DEFAULT_LISTEN_PORT = 51821;

/** VPN page (admin only) — enable the inbound WireGuard server and
 *  grant/revoke/pause per-device remote-access peers (issue #812). */
export default function Vpn() {
  const { data: config, isLoading: configLoading } = useInboundWgConfig();
  const setConfig = useSetInboundWgConfig();
  const { data: peersData, isLoading: peersLoading } = useInboundWgPeers();
  const { data: devicesData } = useDevices();
  const { data: ddns } = useDdnsStatus();
  const addPeer = useAddInboundWgPeer();
  const removePeer = useRemoveInboundWgPeer();
  const setPeerEnabled = useSetInboundWgPeerEnabled();

  const peers = peersData?.peers ?? [];
  const devices = devicesData?.devices ?? [];
  const grantable = devices.filter((d) => d.connection_mode !== "remote");

  // Local override for the listen-port input — `null` until the admin
  // edits it, so the field otherwise tracks the fetched config directly
  // instead of syncing it into state via an effect.
  const [listenPortDraft, setListenPortDraft] = useState<number | null>(null);
  const listenPort =
    listenPortDraft ?? config?.listen_port ?? DEFAULT_LISTEN_PORT;

  const [creating, setCreating] = useState(false);
  const [selectedDeviceId, setSelectedDeviceId] = useState("");
  const [revokeTarget, setRevokeTarget] = useState<InboundWgPeerSummary | null>(
    null,
  );
  const [granted, setGranted] = useState<AddInboundWgPeerResponse | null>(null);

  const enabled = config?.enabled ?? false;

  async function handleGrant() {
    if (!selectedDeviceId) return;
    const response = await addPeer.mutateAsync({
      device_id: selectedDeviceId,
    });
    setGranted(response);
    setCreating(false);
    setSelectedDeviceId("");
  }

  const endpoint = config
    ? placeholderEndpoint(
        config.listen_port,
        ddns?.fqdn ?? null,
        ddns?.last_public_ip ?? null,
      )
    : null;

  return (
    <div className="col gap-20">
      <PageHeader
        title="VPN"
        description="Let devices connect back in from off the LAN through an inbound WireGuard server."
      />

      <Card>
        <CardHeader>
          <CardTitle>Server</CardTitle>
          <CardAction>
            <Toggle
              aria-label="Enable inbound WireGuard server"
              checked={enabled}
              disabled={configLoading || setConfig.isPending}
              onCheckedChange={(next) =>
                setConfig.mutate({ enabled: next, listen_port: listenPort })
              }
            />
          </CardAction>
        </CardHeader>
        <CardContent>
          <div className="flex flex-col gap-2 sm:max-w-xs">
            <Text as="p" size="xs" weight="medium" className="text-ink-3">
              Listen port
            </Text>
            <Input
              type="number"
              min={1}
              max={65535}
              value={listenPort}
              onChange={(e) => setListenPortDraft(Number(e.target.value))}
              disabled={setConfig.isPending}
            />
          </div>
          {config?.server_public_key && (
            <div className="mt-4 flex flex-col gap-1">
              <Text as="p" size="xs" weight="medium" className="text-ink-3">
                Server public key
              </Text>
              <Text as="p" size="xs" className="break-all font-mono">
                {config.server_public_key}
              </Text>
            </div>
          )}
        </CardContent>
        {enabled && listenPort !== config?.listen_port && (
          <CardFooter className="justify-end">
            <Button
              size="sm"
              disabled={setConfig.isPending}
              onClick={() =>
                setConfig.mutate({ enabled: true, listen_port: listenPort })
              }
            >
              {setConfig.isPending ? "Saving…" : "Save port"}
            </Button>
          </CardFooter>
        )}
      </Card>

      {enabled && (
        <Card>
          <CardHeader>
            <CardTitle>Peers</CardTitle>
            <CardAction>
              {!creating && (
                <Button
                  size="sm"
                  onClick={() => setCreating(true)}
                  disabled={grantable.length === 0}
                >
                  Grant access
                </Button>
              )}
            </CardAction>
          </CardHeader>
          <CardContent className="flex flex-col gap-4">
            {creating && (
              <div className="flex flex-col gap-3 rounded-lg border border-line p-4 sm:flex-row sm:items-end">
                <div className="flex-1">
                  <Text
                    as="p"
                    size="xs"
                    weight="medium"
                    className="mb-2 text-ink-3"
                  >
                    Device
                  </Text>
                  <InboundWgDevicePicker
                    devices={grantable}
                    value={selectedDeviceId}
                    onChange={setSelectedDeviceId}
                  />
                </div>
                <div className="flex gap-2">
                  <Button
                    variant="ghost"
                    onClick={() => {
                      setCreating(false);
                      setSelectedDeviceId("");
                    }}
                    disabled={addPeer.isPending}
                  >
                    Cancel
                  </Button>
                  <Button
                    onClick={handleGrant}
                    disabled={!selectedDeviceId || addPeer.isPending}
                  >
                    {addPeer.isPending ? "Granting…" : "Grant"}
                  </Button>
                </div>
              </div>
            )}
            {!peersLoading && (
              <InboundWgPeersTable
                peers={peers}
                onToggleEnabled={(peer) =>
                  setPeerEnabled.mutate({ id: peer.id, enabled: !peer.enabled })
                }
                onRevoke={setRevokeTarget}
              />
            )}
          </CardContent>
        </Card>
      )}

      <ConfirmDialog
        open={!!revokeTarget}
        onOpenChange={(open) => {
          if (!open) setRevokeTarget(null);
        }}
        title="Revoke remote access"
        description={`This permanently deletes ${revokeTarget?.name ?? "this peer"}'s credential. Re-granting later needs a fresh QR scan.`}
        confirmLabel="Revoke"
        onConfirm={() => {
          if (revokeTarget) removePeer.mutate(revokeTarget.id);
          setRevokeTarget(null);
        }}
      />

      <InboundWgPeerGrantedModal
        peer={granted}
        serverPublicKey={config?.server_public_key ?? ""}
        endpoint={endpoint}
        onDismiss={() => setGranted(null)}
      />
    </div>
  );
}
