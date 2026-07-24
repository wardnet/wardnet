import { useState } from "react";
import { PageHeader } from "@/components/compound/PageHeader";
import { TunnelGrid } from "@/components/compound/TunnelGrid";
import type { TunnelTestOutcome } from "@/components/compound/TunnelCard";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import { CreateTunnelInline } from "@/components/features/CreateTunnelInline";
import {
  useTunnels,
  useDeleteTunnel,
  useTestTunnel,
  useRebuildTunnel,
  useStartSpeedTest,
  useSpeedTestResultsList,
} from "@wardnet/web";
import { useProviders } from "@wardnet/web";
import { Button } from "@wardnet/web";

/** Tunnels page for managing WireGuard VPN tunnels (admin only).
 *  Composes the ported `PageHeader`, `TunnelGrid` (Forge `.tcard` family
 *  per `forge/docs/screens.jsx` §03), and `CreateTunnelInline` panel
 *  inside a `col gap-20` page wrapper that matches the studio mock.
 *  Owns the tunnel query and the test/rebuild/speed-test mutations, passing
 *  data + callbacks down to the grid so the cards stay pure presentation and
 *  a single mutation drives the in-flight indicator across the whole list.
 *  Public API unchanged — default-exported, no props (consumed via
 *  `<Route element={<Tunnels />} />` in `App.tsx`). */
export default function Tunnels() {
  const { data, isLoading, isError } = useTunnels();
  const { data: providerData } = useProviders();
  const deleteTunnel = useDeleteTunnel();
  const testTunnel = useTestTunnel();
  const rebuildTunnel = useRebuildTunnel();
  const startSpeedTest = useStartSpeedTest();

  const tunnels = data?.tunnels ?? [];
  const providers = providerData?.providers ?? [];

  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [testOutcomes, setTestOutcomes] = useState<
    Record<string, TunnelTestOutcome>
  >({});
  // Tunnels the user has run a speed test on this session. Each keeps its
  // inline comparison visible; the one still running also shows live progress.
  // Persisted history lives on the tunnel detail page, so the grid only fetches
  // results for cards that were actually tested here.
  const [speedTestIds, setSpeedTestIds] = useState<string[]>([]);

  const speedTestResults = useSpeedTestResultsList(speedTestIds);

  const tunnelToDelete = tunnels.find((t) => t.id === deleteId);
  const hasTunnels = tunnels.length > 0;

  const handleTest = (id: string) => {
    testTunnel.mutate(id, {
      onSuccess: (response) => {
        setTestOutcomes((prev) => ({
          ...prev,
          [id]: {
            result: response.result,
            error: null,
            testedAt: new Date().toISOString(),
          },
        }));
      },
      onError: (err) => {
        setTestOutcomes((prev) => ({
          ...prev,
          [id]: {
            result: null,
            error: err instanceof Error ? err.message : "Tunnel test failed",
            testedAt: new Date().toISOString(),
          },
        }));
      },
    });
  };

  const handleSpeedTest = (id: string) => {
    setSpeedTestIds((prev) => (prev.includes(id) ? prev : [...prev, id]));
    startSpeedTest.start(id);
  };

  return (
    <div className="col gap-20">
      <PageHeader
        title="Tunnels"
        description="WireGuard tunnels you can route device traffic through, e.g. to a VPN provider or your own server."
        actions={
          hasTunnels && !creating ? (
            <Button data-testid="tunnel-add" onClick={() => setCreating(true)}>
              Add tunnel
            </Button>
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
        onTest={handleTest}
        onRebuild={rebuildTunnel.mutate}
        onSpeedTest={handleSpeedTest}
        testOutcomes={testOutcomes}
        speedTestResults={speedTestResults}
        testingId={testTunnel.isPending ? (testTunnel.variables ?? null) : null}
        rebuildingId={
          rebuildTunnel.isPending ? (rebuildTunnel.variables ?? null) : null
        }
        speedTestRunningId={
          startSpeedTest.isRunning ? startSpeedTest.activeTunnelId : null
        }
        speedTestPercentage={startSpeedTest.percentage}
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
