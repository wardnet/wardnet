import { useState } from "react";
import { PageHeader } from "@/components/compound/PageHeader";
import { TunnelGrid } from "@/components/compound/TunnelGrid";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import { CreateTunnelSheet } from "@/components/features/CreateTunnelSheet";
import { useTunnels, useDeleteTunnel } from "@/hooks/useTunnels";
import { useProviders } from "@/hooks/useProviders";
import { Button } from "@/components/core/ui/button";

/** Tunnels page for managing WireGuard VPN tunnels (admin only). */
export default function Tunnels() {
  const { data, isLoading, isError } = useTunnels();
  const { data: providerData } = useProviders();
  const deleteTunnel = useDeleteTunnel();
  const tunnels = data?.tunnels ?? [];
  const providers = providerData?.providers ?? [];
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const tunnelToDelete = tunnels.find((t) => t.id === deleteId);

  const hasTunnels = tunnels.length > 0;

  return (
    <>
      <PageHeader
        title="Tunnels"
        actions={
          hasTunnels ? (
            <Button onClick={() => setCreateOpen(true)}>Add Tunnel</Button>
          ) : undefined
        }
      />

      <TunnelGrid
        tunnels={tunnels}
        providers={providers}
        isLoading={isLoading}
        isError={isError}
        onDelete={setDeleteId}
        onAdd={() => setCreateOpen(true)}
      />

      <CreateTunnelSheet open={createOpen} onOpenChange={setCreateOpen} />

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => { if (!open) setDeleteId(null); }}
        title="Delete tunnel"
        description={`Are you sure you want to delete "${tunnelToDelete?.label ?? "this tunnel"}"? Devices routed through it will be switched to direct.`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteId) deleteTunnel.mutate(deleteId);
          setDeleteId(null);
        }}
      />
    </>
  );
}
