import { useState } from "react";
import { Sheet, SheetContent, SheetTitle } from "@/components/core/ui/sheet";
import { useDevice } from "@/hooks/useDevices";
import { EditDeviceForm } from "./EditDeviceForm";
import { CreateReservationSheet, type ReservationDefaults } from "./CreateReservationSheet";

interface EditDeviceSheetProps {
  deviceId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Sheet for editing a device's name, type, routing, and lock status. */
export function EditDeviceSheet({ deviceId, open, onOpenChange }: EditDeviceSheetProps) {
  const { data } = useDevice(deviceId);
  const device = data?.device;
  const [reservationDefaults, setReservationDefaults] = useState<ReservationDefaults | null>(null);

  return (
    <>
      <Sheet open={open} onOpenChange={onOpenChange}>
        <SheetContent className="w-full overflow-y-auto p-6">
          <SheetTitle>Edit Device</SheetTitle>
          {device ? (
            <EditDeviceForm
              key={device.id + String(data?.current_rule?.type)}
              device={device}
              currentRule={data?.current_rule ?? null}
              onClose={() => onOpenChange(false)}
              onReserveAddress={(mac, ip, hostname) => {
                setReservationDefaults({
                  mac,
                  ip,
                  description: hostname,
                });
              }}
            />
          ) : (
            <p className="mt-6 text-sm text-muted-foreground">Loading device...</p>
          )}
        </SheetContent>
      </Sheet>

      <CreateReservationSheet
        open={!!reservationDefaults}
        onOpenChange={(o) => {
          if (!o) setReservationDefaults(null);
        }}
        defaults={reservationDefaults ?? undefined}
      />
    </>
  );
}
