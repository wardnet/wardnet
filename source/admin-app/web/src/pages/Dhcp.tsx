import { useState } from "react";
import { Card, CardContent } from "@wardnet/forge-web/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@wardnet/forge-web/tabs";
import { PageHeader } from "@/components/compound/PageHeader";
import { DhcpStatusCard } from "@/components/compound/DhcpStatusCard";
import { DhcpConfigCard } from "@/components/compound/DhcpConfigCard";
import { DhcpLeaseTable } from "@/components/compound/DhcpLeaseTable";
import { DhcpReservationTable } from "@/components/compound/DhcpReservationTable";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import type { ReservationDefaults } from "@/components/features/CreateReservationInline";
import { CreateReservationInline } from "@/components/features/CreateReservationInline";
import {
  useDhcpStatus,
  useDhcpConfig,
  useDhcpLeases,
  useDhcpReservations,
  useToggleDhcp,
  useRevokeLease,
  useDeleteReservation,
} from "@/hooks/useDhcp";
import { useDevices } from "@/hooks/useDevices";

/** DHCP management page (admin only). */
export default function Dhcp() {
  const { data: statusData, isLoading: statusLoading } = useDhcpStatus();
  const { data: configData } = useDhcpConfig();
  const { data: leaseData } = useDhcpLeases();
  const { data: reservationData } = useDhcpReservations();
  const { data: deviceData } = useDevices();

  const toggleDhcp = useToggleDhcp();
  const revokeLease = useRevokeLease();
  const deleteReservation = useDeleteReservation();

  const [tab, setTab] = useState("leases");
  const [reservationCreate, setReservationCreate] = useState<{
    open: boolean;
    defaults?: ReservationDefaults;
  }>({ open: false });
  const [revokeLeaseId, setRevokeLeaseId] = useState<string | null>(null);
  const [deleteReservationId, setDeleteReservationId] = useState<string | null>(null);

  const status = statusData;
  const config = configData?.config;
  const leases = leaseData?.leases ?? [];
  const reservations = reservationData?.reservations ?? [];
  const devices = deviceData?.devices ?? [];

  const leaseToRevoke = leases.find((l) => l.id === revokeLeaseId);
  const reservationToDelete = reservations.find((r) => r.id === deleteReservationId);

  function openReservationCreate(defaults?: ReservationDefaults) {
    setTab("reservations");
    setReservationCreate({ open: true, defaults });
  }

  return (
    <>
      <PageHeader title="DHCP" />

      {statusLoading && (
        <Card>
          <CardContent className="py-10 text-center text-ink-3">Loading DHCP status...</CardContent>
        </Card>
      )}

      {status && config && (
        <div className="flex min-h-0 flex-1 flex-col gap-4">
          <div className="grid gap-4 sm:grid-cols-2">
            <DhcpStatusCard
              status={status}
              onToggle={(enabled) => toggleDhcp.mutate(enabled)}
              isPending={toggleDhcp.isPending}
            />
            <DhcpConfigCard config={config} />
          </div>

          <Tabs value={tab} onValueChange={setTab} className="flex min-h-0 flex-1 flex-col">
            <TabsList>
              <TabsTrigger value="leases">Leases</TabsTrigger>
              <TabsTrigger value="reservations">Reservations</TabsTrigger>
            </TabsList>
            <TabsContent value="leases" className="mt-4 flex min-h-0 flex-1 flex-col">
              <DhcpLeaseTable
                leases={leases}
                devices={devices}
                onRevoke={setRevokeLeaseId}
                onMakeStatic={(lease) =>
                  openReservationCreate({
                    mac: lease.mac_address,
                    ip: lease.ip_address,
                    hostname: lease.hostname ?? undefined,
                  })
                }
              />
            </TabsContent>
            <TabsContent value="reservations" className="mt-4 flex min-h-0 flex-1 flex-col gap-4">
              {reservationCreate.open && (
                <CreateReservationInline
                  // Keyed on defaults identity so the form remounts with fresh
                  // initial state whenever it's reopened — same trick the
                  // sheet used, just without the wrapper.
                  key={JSON.stringify(reservationCreate.defaults ?? {})}
                  defaults={reservationCreate.defaults}
                  onClose={() => setReservationCreate({ open: false })}
                />
              )}
              <DhcpReservationTable
                reservations={reservations}
                devices={devices}
                onDelete={setDeleteReservationId}
                onAdd={reservationCreate.open ? undefined : () => openReservationCreate()}
              />
            </TabsContent>
          </Tabs>
        </div>
      )}

      <ConfirmDialog
        open={!!revokeLeaseId}
        onOpenChange={(open) => {
          if (!open) setRevokeLeaseId(null);
        }}
        title="Revoke lease"
        description={`Revoke the lease for ${leaseToRevoke?.ip_address ?? "this device"}${leaseToRevoke?.hostname ? ` (${leaseToRevoke.hostname})` : ""}? The device will need to request a new address.`}
        confirmLabel="Revoke"
        onConfirm={() => {
          if (revokeLeaseId) revokeLease.mutate(revokeLeaseId);
          setRevokeLeaseId(null);
        }}
      />

      <ConfirmDialog
        open={!!deleteReservationId}
        onOpenChange={(open) => {
          if (!open) setDeleteReservationId(null);
        }}
        title="Delete reservation"
        description={`Delete the reservation for ${reservationToDelete?.ip_address ?? "this address"}${reservationToDelete?.description ? ` (${reservationToDelete.description})` : ""}? The MAC address will receive a dynamic IP on next renewal.`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteReservationId) deleteReservation.mutate(deleteReservationId);
          setDeleteReservationId(null);
        }}
      />
    </>
  );
}
