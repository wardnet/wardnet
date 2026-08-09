import { useMemo } from "react";
import { PageHeader } from "@/components/compound/PageHeader";
import { ZonesCard } from "@/components/features/ZonesCard";
import { QuarantineSettingsCard } from "@/components/features/QuarantineSettingsCard";
import { ZoneExceptionsCard } from "@/components/features/ZoneExceptionsCard";
import {
  useDevices,
  usePendingDevices,
  useZoneExceptions,
  useCreateNetworkZone,
  useUpdateNetworkZone,
  useDeleteNetworkZone,
  useCreateZoneException,
  useDeleteZoneException,
  useQuarantineNewDevices,
  useSetQuarantineNewDevices,
  useAssignDeviceZone,
} from "@wardnet/web";

/** Network Zones page (admin only) — device policy buckets, cross-zone
 *  exceptions/casting, and new-device quarantine. Epic #244.
 *  Owns every query and mutation for the three cards below, passing data +
 *  callbacks down so the cards stay pure presentation and the zone/device
 *  queries are fetched once for the whole page. */
export default function Zones() {
  // Zone lifecycle. `usePendingDevices` composes the zone + device queries the
  // whole page shares (TanStack dedupes them by query key).
  const { pending, defaultForNew, homeZone, zones } = usePendingDevices();
  const createZone = useCreateNetworkZone();
  const updateZone = useUpdateNetworkZone();
  const setHome = useUpdateNetworkZone({ successMessage: "Home zone updated" });
  const setDefaultForNew = useUpdateNetworkZone({
    successMessage: "Default-for-new zone updated",
  });
  const deleteZone = useDeleteNetworkZone();

  // Cross-zone exceptions.
  const { data: exceptionData } = useZoneExceptions();
  const { data: deviceData } = useDevices();
  const createException = useCreateZoneException();
  const deleteException = useDeleteZoneException();

  // New-device quarantine.
  const { data: quarantine } = useQuarantineNewDevices();
  const setQuarantine = useSetQuarantineNewDevices();
  const approveDevice = useAssignDeviceZone({
    successMessage: "Device approved",
  });

  const devices = useMemo(() => deviceData?.devices ?? [], [deviceData]);
  const exceptions = useMemo(
    () => exceptionData?.exceptions ?? [],
    [exceptionData],
  );

  return (
    <div className="col gap-20">
      <PageHeader
        title="Zones"
        description="Group devices into policy buckets that gate routing and isolate them from the rest of your network."
      />

      <div className="col gap-20">
        <ZonesCard
          zones={zones}
          isSaving={createZone.isPending || updateZone.isPending}
          onCreateZone={createZone.mutate}
          onUpdateZone={updateZone.mutate}
          onSetHome={(id) => setHome.mutate({ id, body: { is_default: true } })}
          onSetDefaultForNew={(id) =>
            setDefaultForNew.mutate({ id, body: { is_default_for_new: true } })
          }
          onDeleteZone={deleteZone.mutate}
        />
        <ZoneExceptionsCard
          exceptions={exceptions}
          zones={zones}
          devices={devices}
          isSaving={createException.isPending}
          onCreateException={createException.mutate}
          onDeleteException={deleteException.mutate}
        />
        <QuarantineSettingsCard
          notifyEnabled={quarantine?.enabled ?? false}
          notifyPending={setQuarantine.isPending}
          onSetNotify={setQuarantine.mutate}
          onSetDefaultForNew={(id) =>
            setDefaultForNew.mutate({ id, body: { is_default_for_new: true } })
          }
          pending={pending}
          defaultForNew={defaultForNew}
          homeZone={homeZone}
          zones={zones}
          onApprove={(deviceId, zoneId) =>
            approveDevice.mutate({ deviceId, zoneId })
          }
          approvingDeviceId={
            approveDevice.isPending
              ? (approveDevice.variables?.deviceId ?? null)
              : null
          }
        />
      </div>
    </div>
  );
}
