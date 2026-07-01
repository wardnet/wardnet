import { useState } from "react";
import { FormActions } from "@wardnet/web";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Ipv4Input } from "@/components/core/ui/ipv4-input";
import { MacInput } from "@/components/core/ui/mac-input";
import { ApiErrorAlert } from "@wardnet/web";
import { useCreateReservation } from "@wardnet/web";

/** Optional pre-filled values for the reservation form. */
export interface ReservationDefaults {
  mac?: string;
  ip?: string;
  hostname?: string;
  description?: string;
}

interface CreateReservationInlineProps {
  /** Called on Cancel or after a successful create. */
  onClose: () => void;
  /** Called after a successful create (before `onClose`). The parent
   *  uses this to switch the group filter to "reservations" — we
   *  intentionally don't change the active group on form open so that
   *  pre-filled defaults from "Make static" don't yank the user out
   *  of the leases view. */
  onSuccess?: () => void;
  defaults?: ReservationDefaults;
}

/**
 * Inline panel for creating a DHCP reservation.
 *
 * Replaces the previous sheet-based flow to match the
 * `CreateTunnelInline` / `DeviceSettingsCard` inline-edit pattern: the
 * Dhcp page toggles this card into existence above the entry table
 * rather than sliding in a side panel.
 *
 * Initial values come from `defaults` on first mount. The parent is
 * expected to remount this component when defaults change (typically
 * by giving it a `key` tied to `defaults`), which keeps state init in
 * `useState` instead of cascading via `useEffect`.
 */
export function CreateReservationInline({
  onClose,
  onSuccess,
  defaults,
}: CreateReservationInlineProps) {
  const createReservation = useCreateReservation();

  const [macAddress, setMacAddress] = useState(defaults?.mac ?? "");
  const [ipAddress, setIpAddress] = useState(defaults?.ip ?? "");
  const [hostname, setHostname] = useState(defaults?.hostname ?? "");
  const [description, setDescription] = useState(defaults?.description ?? "");

  async function handleSave() {
    await createReservation.mutateAsync({
      mac_address: macAddress,
      ip_address: ipAddress,
      hostname: hostname || undefined,
      description: description || undefined,
    });
    onSuccess?.();
    onClose();
  }

  const macReadOnly = !!defaults?.mac;

  return (
    <Card>
      <CardHeader>
        <CardTitle>
          {defaults?.mac ? "Reserve address" : "Add reservation"}
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        <div className="flex gap-3">
          <Field label="MAC address" htmlFor="res-mac" className="flex-1">
            <MacInput
              id="res-mac"
              data-testid="dhcp-reservation-mac"
              value={macAddress}
              onChange={setMacAddress}
              readOnly={macReadOnly}
            />
          </Field>

          <Field label="IP address" htmlFor="res-ip" className="flex-1">
            <Ipv4Input
              id="res-ip"
              data-testid="dhcp-reservation-ip"
              value={ipAddress}
              onChange={setIpAddress}
              placeholder="10.232.1.50"
            />
          </Field>
        </div>

        <div className="flex gap-3">
          <Field
            label="Hostname (optional)"
            htmlFor="res-hostname"
            className="flex-1"
          >
            <Input
              id="res-hostname"
              data-testid="dhcp-reservation-hostname"
              value={hostname}
              onChange={(e) => setHostname(e.target.value)}
              placeholder="my-printer"
            />
          </Field>

          <Field
            label="Description (optional)"
            htmlFor="res-desc"
            className="flex-1"
          >
            <Input
              id="res-desc"
              data-testid="dhcp-reservation-desc"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Office printer"
            />
          </Field>
        </div>

        {createReservation.isError && (
          <ApiErrorAlert
            error={createReservation.error}
            fallback="Failed to create reservation"
          />
        )}
      </CardContent>
      <FormActions
        secondaryLabel="Cancel"
        secondaryProps={{
          onClick: onClose,
          disabled: createReservation.isPending,
          "data-testid": "dhcp-reservation-cancel",
        }}
        primaryLabel={
          createReservation.isPending ? "Creating…" : "Create reservation"
        }
        primaryProps={{
          onClick: handleSave,
          disabled: createReservation.isPending,
          "data-testid": "dhcp-reservation-submit",
        }}
      />
    </Card>
  );
}
