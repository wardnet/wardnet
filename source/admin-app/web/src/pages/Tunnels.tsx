import { useState } from "react";
import { PageHeader } from "@/components/compound/PageHeader";
import { TunnelGrid } from "@/components/compound/TunnelGrid";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import { CreateTunnelInline } from "@/components/features/CreateTunnelInline";
import { useTunnels, useDeleteTunnel } from "@/hooks/useTunnels";
import { useProviders } from "@/hooks/useProviders";
import { Button } from "@wardnet/forge-web/button";

/** Tunnels page for managing WireGuard VPN tunnels (admin only).
 *  Composes the ported `PageHeader`, `TunnelGrid` (Forge `.tcard` family
 *  per `forge/docs/screens.jsx` §03), and `CreateTunnelInline` panel
 *  inside a `col gap-20` page wrapper that matches the studio mock.
 *  Public API unchanged — default-exported, no props (consumed via
 *  `<Route element={<Tunnels />} />` in `App.tsx`). */
export default function Tunnels() {
  const { data, isLoading, isError } = useTunnels();
  const { data: providerData } = useProviders();
  const deleteTunnel = useDeleteTunnel();
  const tunnels = data?.tunnels ?? [];
  const providers = providerData?.providers ?? [];
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const tunnelToDelete = tunnels.find((t) => t.id === deleteId);

  const hasTunnels = tunnels.length > 0;

  return (
    <div className="col gap-20">
      <PageHeader
        title="Tunnels"
        actions={
          hasTunnels && !creating ? (
            <Button onClick={() => setCreating(true)}>Add tunnel</Button>
          ) : undefined
        }
      />

      {creating && <CreateTunnelInline onClose={() => setCreating(false)} />}

      <TunnelGrid
        tunnels={tunnels}
        providers={providers}
        isLoading={isLoading}
        isError={isError}
        onDelete={setDeleteId}
        onAdd={creating ? undefined : () => setCreating(true)}
      />

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => {
          if (!open) setDeleteId(null);
        }}
        title="Delete tunnel"
        description={`Are you sure you want to delete "${tunnelToDelete?.label ?? "this tunnel"}"? Devices routed through it will be switched to direct.`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteId) deleteTunnel.mutate(deleteId);
          setDeleteId(null);
        }}
      />
    </div>
  );
}
