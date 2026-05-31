import { useState } from "react";
import { Button } from "@wardnet/forge-web/button";
import {
  Card,
  CardAction,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@wardnet/forge-web/card";
import { Field } from "@wardnet/forge-web/field";
import { Ipv4Input } from "@/components/core/ui/ipv4-input";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { StatusBadge } from "@/components/compound/StatusBadge";
import {
  useCreateReservation,
  useDeleteReservation,
  useDhcpReservations,
} from "@wardnet/wardnet-web";
import type { Device, DhcpReservation, DhcpStatus } from "@wardnet/js";

interface DeviceNetworkCardProps {
  device: Device;
}

function statusBadge(status: DhcpStatus) {
  switch (status) {
    case "lease":
      return <StatusBadge tone="success">Lease</StatusBadge>;
    case "reservation":
      return <StatusBadge tone="success">Reserved</StatusBadge>;
    case "external":
      return <StatusBadge tone="neutral">External</StatusBadge>;
  }
}

function findReservation(
  reservations: DhcpReservation[] | undefined,
  mac: string,
): DhcpReservation | undefined {
  // MAC is canonical lowercase server-side (issue #312); both sides are
  // already in the same form.
  return reservations?.find((r) => r.mac_address === mac);
}

/** Editable network card: current IP, DHCP status, reservation create/update/remove. */
export function DeviceNetworkCard({ device }: DeviceNetworkCardProps) {
  const { data: reservationsData } = useDhcpReservations();
  const reservation = findReservation(reservationsData?.reservations, device.mac);

  const createReservation = useCreateReservation();
  const deleteReservation = useDeleteReservation();

  const [editing, setEditing] = useState(false);
  const [ip, setIp] = useState(reservation?.ip_address ?? device.last_ip);

  function startEdit() {
    setIp(reservation?.ip_address ?? device.last_ip);
    createReservation.reset();
    deleteReservation.reset();
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    createReservation.reset();
    deleteReservation.reset();
  }

  async function handleSave() {
    // No update endpoint exists — replacing an existing reservation is a
    // delete-then-create. Skip the cycle when the IP is unchanged.
    if (reservation) {
      if (reservation.ip_address === ip) {
        setEditing(false);
        return;
      }
      await deleteReservation.mutateAsync(reservation.id);
    }
    await createReservation.mutateAsync({
      mac_address: device.mac,
      ip_address: ip,
      hostname: device.hostname ?? undefined,
      description: device.name ?? device.hostname ?? undefined,
    });
    setEditing(false);
  }

  async function handleRemove() {
    if (!reservation) return;
    await deleteReservation.mutateAsync(reservation.id);
    setEditing(false);
  }

  const busy = createReservation.isPending || deleteReservation.isPending;
  const error = createReservation.error ?? deleteReservation.error;
  const isError = createReservation.isError || deleteReservation.isError;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Network</CardTitle>
        {!editing && (
          <CardAction>
            <Button variant="outline" size="sm" onClick={startEdit}>
              Edit
            </Button>
          </CardAction>
        )}
      </CardHeader>

      {editing ? (
        <>
          <CardContent className="flex flex-col gap-5">
            <Field
              label="IP address"
              htmlFor="device-ip"
              help="Saving creates a static DHCP reservation for this device's MAC."
            >
              <Ipv4Input id="device-ip" value={ip} onChange={setIp} />
            </Field>

            {isError && <ApiErrorAlert error={error} fallback="Failed to update reservation" />}
          </CardContent>
          <CardFooter className="gap-2">
            {reservation && (
              <Button
                variant="destructive"
                size="sm"
                onClick={handleRemove}
                disabled={busy}
                className="mr-auto"
              >
                Remove reservation
              </Button>
            )}
            <Button variant="ghost" onClick={cancelEdit} disabled={busy} className="ml-auto">
              Cancel
            </Button>
            <Button onClick={handleSave} disabled={busy}>
              {busy ? "Saving…" : "Save"}
            </Button>
          </CardFooter>
        </>
      ) : (
        <CardContent className="grid grid-cols-2 gap-x-6 gap-y-4">
          <div className="flex flex-col gap-0.5">
            <span className="text-xs uppercase tracking-wide text-ink-3">IP</span>
            <span className="font-mono text-sm">{device.last_ip}</span>
          </div>
          <div className="flex flex-col gap-0.5">
            <span className="text-xs uppercase tracking-wide text-ink-3">DHCP</span>
            <span className="text-sm">{statusBadge(device.dhcp_status)}</span>
          </div>
        </CardContent>
      )}
    </Card>
  );
}
