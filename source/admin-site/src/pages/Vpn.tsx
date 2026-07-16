import { useMemo, useState } from "react";
import { Link } from "react-router";
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
  InboundWgBetaNotice,
} from "@wardnet/web";
import { PageHeader } from "@/components/compound/PageHeader";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import { DeviceSelect } from "@/components/compound/DeviceSelect";
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
  const addPeer = useAddInboundWgPeer();
  const removePeer = useRemoveInboundWgPeer();
  const setPeerEnabled = useSetInboundWgPeerEnabled();

  // Memoised so the `?? []` fallback cannot mint a fresh array identity on
  // every render: the derived lists below depend on these, and DeviceSelect
  // memoises its alphabetical sort on the `devices` prop it is handed.
  const peers = useMemo(() => peersData?.peers ?? [], [peersData?.peers]);
  const devices = useMemo(
    () => devicesData?.devices ?? [],
    [devicesData?.devices],
  );
  // Only *managed* (admin-named) devices can be granted remote access — the
  // backend rejects unmanaged ones. A device with no name is still just
  // "discovered".
  const managedDevices = useMemo(
    () => devices.filter((d) => d.name != null),
    [devices],
  );
  // Grantable = managed AND has no peer row yet. `connection_mode` is NOT a
  // reliable signal: the daemon only flips it to `remote` once the peer
  // actually handshakes (and clears it again on pause), so an offline /
  // freshly granted / paused device would still read `!== "remote"` and get
  // offered for a re-grant that the one-credential-per-device guard 409s.
  // Memoised so the array identity is stable across renders: DeviceSelect
  // memoises its alphabetical sort on the `devices` prop, and a fresh array
  // every render would defeat that.
  const grantable = useMemo(() => {
    const grantedDeviceIds = new Set(
      peers.map((p) => p.device_id).filter((id): id is string => id !== null),
    );
    return managedDevices.filter((d) => !grantedDeviceIds.has(d.id));
  }, [managedDevices, peers]);

  // Local override for the listen-port input — `null` until the admin
  // edits it, so the field otherwise tracks the fetched config directly
  // instead of syncing it into state via an effect.
  const [listenPortDraft, setListenPortDraft] = useState<number | null>(null);
  const listenPort =
    listenPortDraft ?? config?.listen_port ?? DEFAULT_LISTEN_PORT;
  // `Number("")` is 0, so a cleared field yields 0 (a value `??` does not
  // coalesce and `min={1}` does not block). Gate every mutation on a valid
  // port so the server is never (re)configured onto port 0.
  const portValid =
    Number.isInteger(listenPort) && listenPort >= 1 && listenPort <= 65535;

  const [creating, setCreating] = useState(false);
  const [selectedDeviceId, setSelectedDeviceId] = useState("");
  const [revokeTarget, setRevokeTarget] = useState<InboundWgPeerSummary | null>(
    null,
  );
  const [granted, setGranted] = useState<AddInboundWgPeerResponse | null>(null);

  const enabled = config?.enabled ?? false;

  async function handleGrant() {
    if (!selectedDeviceId) return;
    try {
      const response = await addPeer.mutateAsync({
        device_id: selectedDeviceId,
      });
      setGranted(response);
      setCreating(false);
      setSelectedDeviceId("");
    } catch {
      // The mutation's onError already surfaced a toast; keep the panel open
      // (with the selection) so the admin can retry or pick another device,
      // and swallow the rejection so it isn't an unhandled promise rejection.
    }
  }

  return (
    <div className="col gap-20">
      <PageHeader
        title="VPN"
        description="Let devices connect back in from off the LAN through an inbound WireGuard server."
      />

      <InboundWgBetaNotice />

      <Card>
        <CardHeader>
          <CardTitle>Server</CardTitle>
          <CardAction>
            <Toggle
              aria-label="Enable inbound WireGuard server"
              checked={enabled}
              // Block enabling with an invalid port; disabling is always
              // allowed (the port is irrelevant when turning the server off).
              disabled={
                configLoading || setConfig.isPending || (!enabled && !portValid)
              }
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
        </CardContent>
        {enabled && listenPort !== config?.listen_port && (
          <CardFooter className="justify-end">
            <Button
              size="sm"
              disabled={setConfig.isPending || !portValid}
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
              {!creating && grantable.length > 0 && (
                <Button size="sm" onClick={() => setCreating(true)}>
                  Grant access
                </Button>
              )}
            </CardAction>
          </CardHeader>
          <CardContent className="flex flex-col gap-4">
            {managedDevices.length === 0 && (
              <Text as="p" size="sm" className="text-ink-3">
                Only managed devices can be granted remote access. Give a device
                a name on the{" "}
                <Link to="/devices" className="text-accent hover:underline">
                  Devices
                </Link>{" "}
                page to manage it, then grant it access here.
              </Text>
            )}
            {managedDevices.length > 0 &&
              grantable.length === 0 &&
              !creating && (
                <Text as="p" size="sm" className="text-ink-3">
                  Every managed device already has remote access. Name another
                  device on the{" "}
                  <Link to="/devices" className="text-accent hover:underline">
                    Devices
                  </Link>{" "}
                  page to grant more.
                </Text>
              )}
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
                  <DeviceSelect
                    devices={grantable}
                    value={selectedDeviceId}
                    onChange={setSelectedDeviceId}
                    valueKey="id"
                    includeAny={false}
                    anyLabel="Choose a device"
                    emptyLabel="Every managed device already has remote access."
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
        onDismiss={() => setGranted(null)}
      />
    </div>
  );
}
