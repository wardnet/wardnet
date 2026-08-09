import { useState } from "react";
import { Button } from "@wardnet/web";
import { FormActions } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Ipv4Input } from "@wardnet/web";
import { ApiErrorAlert } from "@wardnet/web";
import { StatusBadge } from "@/components/compound/StatusBadge";
import type { MutationHandle } from "@/lib/mutationHandle";
import type {
  CreateDhcpReservationRequest,
  Device,
  DhcpReservation,
  DhcpStatus,
} from "@wardnet/js";

interface DeviceNetworkCardProps {
  device: Device;
  /** All DHCP reservations — the card resolves this device's by MAC. */
  reservations: DhcpReservation[] | undefined;
  /** The page's hoisted create-reservation mutation. */
  createReservation: MutationHandle<CreateDhcpReservationRequest>;
  /** Dedicated silent create instance for the rollback recreate: it must not
   *  pop a "Reservation created" toast that would contradict the failure the
   *  admin is being shown for their actual edit. */
  restoreReservation: MutationHandle<CreateDhcpReservationRequest>;
  /** The page's hoisted delete-reservation mutation. */
  deleteReservation: MutationHandle<string>;
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

/** Editable network card: current IP, DHCP status, reservation
 *  create/update/remove. Pure presentation — the owning page wires the
 *  query/mutation hooks and passes data + callbacks in. */
export function DeviceNetworkCard({
  device,
  reservations,
  createReservation,
  restoreReservation,
  deleteReservation,
}: DeviceNetworkCardProps) {
  const reservation = findReservation(reservations, device.mac);

  const [editing, setEditing] = useState(false);
  const [ip, setIp] = useState(reservation?.ip_address ?? device.last_ip);
  // Error captured from a failed replacement whose rollback recreate then
  // succeeded — the recreate clears the create mutation's own error state, so
  // we hold onto the original failure to keep surfacing it to the admin.
  const [saveError, setSaveError] = useState<unknown>(null);

  function startEdit() {
    setIp(reservation?.ip_address ?? device.last_ip);
    setSaveError(null);
    createReservation.reset();
    restoreReservation.reset();
    deleteReservation.reset();
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    setSaveError(null);
    createReservation.reset();
    restoreReservation.reset();
    deleteReservation.reset();
  }

  async function handleSave() {
    setSaveError(null);
    // No update endpoint exists — replacing an existing reservation is a
    // delete-then-create. Skip the cycle when the IP is unchanged.
    if (reservation) {
      if (reservation.ip_address === ip) {
        setEditing(false);
        return;
      }
      await deleteReservation.mutateAsync(reservation.id);
      try {
        await createReservation.mutateAsync({
          mac_address: device.mac,
          ip_address: ip,
          hostname: device.hostname ?? undefined,
          description: device.name ?? device.hostname ?? undefined,
        });
      } catch (err) {
        // Create failed after the old reservation was already deleted (e.g.
        // the new IP is taken, 409). Recreate the original so the device
        // doesn't silently fall back to a dynamic lease, and keep the
        // failure on screen so the admin knows the edit didn't take.
        setSaveError(err);
        try {
          await restoreReservation.mutateAsync({
            mac_address: reservation.mac_address,
            ip_address: reservation.ip_address,
            hostname: reservation.hostname ?? undefined,
            description: reservation.description ?? undefined,
          });
        } catch {
          // The restore itself failed — worse than the original error, since
          // the device now has no static reservation at all. Replace the
          // message with one that says so, rather than leaving the admin with
          // the (now-misleading) new-IP failure.
          setSaveError(
            new Error(
              `Could not restore the previous reservation (${reservation.ip_address}). ` +
                "This device no longer has a static reservation and will fall " +
                "back to a dynamic DHCP lease — re-add it to restore a fixed address.",
            ),
          );
        }
        return;
      }
      setEditing(false);
      return;
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

  const busy =
    createReservation.isPending ||
    restoreReservation.isPending ||
    deleteReservation.isPending;
  const error = saveError ?? createReservation.error ?? deleteReservation.error;
  const isError =
    saveError != null || createReservation.isError || deleteReservation.isError;

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

            {isError && (
              <ApiErrorAlert
                error={error}
                fallback="Failed to update reservation"
              />
            )}
          </CardContent>
          <FormActions
            tertiaryLabel={reservation ? "Remove reservation" : undefined}
            tertiaryVariant="destructive"
            tertiaryProps={{ onClick: handleRemove, disabled: busy }}
            secondaryLabel="Cancel"
            secondaryProps={{ onClick: cancelEdit, disabled: busy }}
            primaryLabel={busy ? "Saving…" : "Save"}
            primaryProps={{ onClick: handleSave, disabled: busy }}
          />
        </>
      ) : (
        <CardContent className="grid grid-cols-2 gap-x-6 gap-y-4">
          <div className="flex flex-col gap-0.5">
            <Text size="xs" className="uppercase tracking-wide text-ink-3">
              IP
            </Text>
            <Text size="sm" className="font-mono">
              {device.last_ip}
            </Text>
          </div>
          <div className="flex flex-col gap-0.5">
            <Text size="xs" className="uppercase tracking-wide text-ink-3">
              DHCP
            </Text>
            <Text size="sm">{statusBadge(device.dhcp_status)}</Text>
          </div>
        </CardContent>
      )}
    </Card>
  );
}
