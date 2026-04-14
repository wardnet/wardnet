import { useState } from "react";
import { Button } from "@/components/core/ui/button";
import { Input } from "@/components/core/ui/input";
import { Ipv4Input } from "@/components/core/ui/ipv4-input";
import { MacInput } from "@/components/core/ui/mac-input";
import { Label } from "@/components/core/ui/label";
import { Sheet, SheetContent, SheetTitle } from "@/components/core/ui/sheet";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { useCreateReservation } from "@/hooks/useDhcp";

/** Optional pre-filled values for the reservation form. */
export interface ReservationDefaults {
  mac?: string;
  ip?: string;
  hostname?: string;
  description?: string;
}

interface CreateReservationSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  defaults?: ReservationDefaults;
}

/** Sheet form for creating a new DHCP reservation. Accepts optional pre-filled values. */
export function CreateReservationSheet({
  open,
  onOpenChange,
  defaults,
}: CreateReservationSheetProps) {
  const createReservation = useCreateReservation();

  // Initial values are taken from `defaults` on first mount. The parent is
  // expected to re-mount this component whenever it needs fresh defaults
  // (typically by giving the `<Sheet>` a key tied to `defaults`), which is
  // cleaner than syncing state via useEffect and avoids cascading renders.
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
    onOpenChange(false);
  }

  const macReadOnly = !!defaults?.mac;

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full overflow-y-auto p-6">
        <SheetTitle>{defaults?.mac ? "Reserve Address" : "Add Reservation"}</SheetTitle>
        <div className="mt-6 flex flex-col gap-5">
          <div className="flex flex-col gap-2">
            <Label htmlFor="res-mac">MAC Address</Label>
            <MacInput
              id="res-mac"
              value={macAddress}
              onChange={setMacAddress}
              readOnly={macReadOnly}
            />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="res-ip">IP Address</Label>
            <Ipv4Input
              id="res-ip"
              value={ipAddress}
              onChange={setIpAddress}
              placeholder="10.232.1.50"
            />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="res-hostname">Hostname (optional)</Label>
            <Input
              id="res-hostname"
              value={hostname}
              onChange={(e) => setHostname(e.target.value)}
              placeholder="my-printer"
            />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="res-desc">Description (optional)</Label>
            <Input
              id="res-desc"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Office printer"
            />
          </div>

          {createReservation.isError && (
            <ApiErrorAlert
              error={createReservation.error}
              fallback="Failed to create reservation"
            />
          )}

          <Button onClick={handleSave} disabled={createReservation.isPending} className="w-full">
            {createReservation.isPending ? "Creating..." : "Create Reservation"}
          </Button>
        </div>
      </SheetContent>
    </Sheet>
  );
}
