import { useMemo } from "react";
import { Card, CardContent } from "@wardnet/web";
import { sortByLabel } from "@wardnet/web";
import { TunnelCard } from "./TunnelCard";
import type { TunnelTestOutcome } from "./TunnelCard";
import { EmptyStatePlaceholder } from "./EmptyStatePlaceholder";
import type { Tunnel, ProviderInfo, TunnelSpeedTestResult } from "@wardnet/js";

interface TunnelGridProps {
  /** Rendered alphabetically by label regardless of the order passed in. */
  tunnels: Tunnel[];
  providers: ProviderInfo[];
  isLoading: boolean;
  isError: boolean;
  onDelete: (id: string) => void;
  onTest: (id: string) => void;
  onRebuild: (id: string) => void;
  onSpeedTest: (id: string) => void;
  /** Connectivity-test outcomes keyed by tunnel id. */
  testOutcomes: Record<string, TunnelTestOutcome>;
  /** Latest speed-test result keyed by tunnel id. */
  speedTestResults: Record<string, TunnelSpeedTestResult | null>;
  /** Tunnel whose connectivity test is in flight, if any. */
  testingId: string | null;
  /** Tunnel whose rebuild is in flight, if any. */
  rebuildingId: string | null;
  /** Tunnel whose speed test is running, if any. */
  speedTestRunningId: string | null;
  /** Progress of the running speed test (0 when none is running). */
  speedTestPercentage: number;
  /** Called when the user clicks the "Add tunnel" button in the empty state. */
  onAdd?: () => void;
}

/** Responsive grid of tunnel cards with loading/empty states. */
export function TunnelGrid({
  tunnels,
  providers,
  isLoading,
  isError,
  onDelete,
  onTest,
  onRebuild,
  onSpeedTest,
  testOutcomes,
  speedTestResults,
  testingId,
  rebuildingId,
  speedTestRunningId,
  speedTestPercentage,
  onAdd,
}: TunnelGridProps) {
  // Before the loading/empty early-returns: hook order must not depend on state.
  const sorted = useMemo(() => sortByLabel(tunnels, (t) => t.label), [tunnels]);

  if (isLoading) {
    return (
      <Card>
        <CardContent className="py-10 text-center text-ink-3">
          Loading tunnels...
        </CardContent>
      </Card>
    );
  }

  if (!isError && tunnels.length === 0) {
    return (
      <EmptyStatePlaceholder
        message="No tunnels configured"
        hint="Add a WireGuard tunnel to route device traffic through a VPN provider."
        actionLabel={onAdd ? "Add tunnel" : undefined}
        onAction={onAdd}
        actionTestId="tunnel-add"
      />
    );
  }

  if (tunnels.length === 0) return null;

  return (
    <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
      {sorted.map((tunnel) => {
        // `tunnel.id` is the card's own key and both maps are page-owned, so
        // these are read-only lookups with a trusted key. (This used to carry
        // `security/detect-object-injection` suppressions; the rule no longer
        // flags them, and leaving the directives in place made eslint report
        // them as unused.)
        const testOutcome = testOutcomes[tunnel.id] ?? null;
        const speedTestResult = speedTestResults[tunnel.id] ?? null;
        return (
          <TunnelCard
            key={tunnel.id}
            tunnel={tunnel}
            providers={providers}
            onDelete={onDelete}
            onTest={onTest}
            onRebuild={onRebuild}
            onSpeedTest={onSpeedTest}
            testOutcome={testOutcome}
            speedTestResult={speedTestResult}
            testing={testingId === tunnel.id}
            rebuilding={rebuildingId === tunnel.id}
            speedTestRunning={speedTestRunningId === tunnel.id}
            speedTestPercentage={
              speedTestRunningId === tunnel.id ? speedTestPercentage : 0
            }
          />
        );
      })}
    </div>
  );
}
