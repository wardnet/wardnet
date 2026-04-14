import { useState } from "react";
import { Card, CardContent } from "@/components/core/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/core/ui/tabs";
import { PageHeader } from "@/components/compound/PageHeader";
import { DhcpStatusCard } from "@/components/compound/DhcpStatusCard";
import { DhcpConfigCard } from "@/components/compound/DhcpConfigCard";
import { DhcpLeaseTable } from "@/components/compound/DhcpLeaseTable";
import { DhcpReservationTable } from "@/components/compound/DhcpReservationTable";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import { EditDhcpConfigSheet } from "@/components/features/EditDhcpConfigSheet";
import type { ReservationDefaults } from "@/components/features/CreateReservationSheet";
import { CreateReservationSheet } from "@/components/features/CreateReservationSheet";
import {
  useDhcpStatus,
  useDhcpConfig,
  useDhcpLeases,
  useDhcpReservations,
  useToggleDhcp,
  useRevokeLease,
  useDeleteReservation,
} from "@/hooks/useDhcp";

/** DHCP management page (admin only). */
export default function Dhcp() {
  const { data: statusData, isLoading: statusLoading } = useDhcpStatus();
  const { data: configData } = useDhcpConfig();
  const { data: leaseData } = useDhcpLeases();
  const { data: reservationData } = useDhcpReservations();

  const toggleDhcp = useToggleDhcp();
  const revokeLease = useRevokeLease();
  const deleteReservation = useDeleteReservation();

  const [editConfigOpen, setEditConfigOpen] = useState(false);
  const [reservationSheet, setReservationSheet] = useState<{
    open: boolean;
    defaults?: ReservationDefaults;
  }>({ open: false });
  const [revokeLeaseId, setRevokeLeaseId] = useState<string | null>(null);
  const [deleteReservationId, setDeleteReservationId] = useState<string | null>(null);

  const status = statusData;
  const config = configData?.config;
  const leases = leaseData?.leases ?? [];
  const reservations = reservationData?.reservations ?? [];

  const leaseToRevoke = leases.find((l) => l.id === revokeLeaseId);
  const reservationToDelete = reservations.find((r) => r.id === deleteReservationId);

  return (
    <>
      <PageHeader title="DHCP" />

      {statusLoading && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            Loading DHCP status...
          </CardContent>
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
            <DhcpConfigCard config={config} onEdit={() => setEditConfigOpen(true)} />
          </div>

          <Tabs defaultValue="leases" className="flex min-h-0 flex-1 flex-col">
            <TabsList>
              <TabsTrigger value="leases">Leases</TabsTrigger>
              <TabsTrigger value="reservations">Reservations</TabsTrigger>
            </TabsList>
            <TabsContent value="leases" className="mt-4 flex min-h-0 flex-1 flex-col">
              <DhcpLeaseTable
                leases={leases}
                onRevoke={setRevokeLeaseId}
                onMakeStatic={(lease) =>
                  setReservationSheet({
                    open: true,
                    defaults: {
                      mac: lease.mac_address,
                      ip: lease.ip_address,
                      hostname: lease.hostname ?? undefined,
                    },
                  })
                }
              />
            </TabsContent>
            <TabsContent value="reservations" className="mt-4 flex min-h-0 flex-1 flex-col">
              <DhcpReservationTable
                reservations={reservations}
                onDelete={setDeleteReservationId}
                onAdd={() => setReservationSheet({ open: true })}
              />
            </TabsContent>
          </Tabs>

          <EditDhcpConfigSheet
            config={config}
            open={editConfigOpen}
            onOpenChange={setEditConfigOpen}
          />
          <CreateReservationSheet
            // Keyed on the defaults identity so the sheet mounts fresh each
            // time it's opened — this lets the component's initial useState
            // values pick up the latest defaults without a useEffect sync.
            key={reservationSheet.open ? JSON.stringify(reservationSheet.defaults ?? {}) : "closed"}
            open={reservationSheet.open}
            onOpenChange={(o) => {
              if (!o) setReservationSheet({ open: false });
            }}
            defaults={reservationSheet.defaults}
          />
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
