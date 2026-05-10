import { useState } from "react";
import { Button } from "@wardnet/forge-web/button";
import { RoutingSelector } from "@/components/compound/RoutingSelector";
import { useAdvanceWizard } from "@/hooks/useSetup";
import { useDefaultPolicy, useSetDefaultPolicy } from "@/hooks/useDefaultPolicy";
import { useTunnels } from "@/hooks/useTunnels";
import type { RoutingTarget } from "@wardnet/js";

/**
 * Step 6 — pick the global default routing policy.
 *
 * Reuses the same `RoutingSelector` compound that powers the per-device
 * routing dropdown so the wizard's "default" picker visually matches what
 * the operator will see on Devices later. The selector deals in
 * `RoutingTarget` shapes (`{type: "direct"}` | `{type: "tunnel", ...}`);
 * the daemon stores `default_policy` as a plain string (`"direct"` or a
 * tunnel UUID) so we adapt at the edges.
 */
function policyToTarget(policy: string): RoutingTarget {
  if (policy === "direct") return { type: "direct" };
  return { type: "tunnel", tunnel_id: policy };
}

function targetToPolicy(target: RoutingTarget): string {
  if (target.type === "tunnel") return target.tunnel_id;
  // `default` collapses to direct here — the wizard never offers it
  // and the selector never emits it.
  return "direct";
}

export default function Step6Policy() {
  const advance = useAdvanceWizard();
  const setDefault = useSetDefaultPolicy();
  const { data: current } = useDefaultPolicy();
  const { data: tunnels } = useTunnels();
  const tunnelList = tunnels?.tunnels ?? [];

  // Selector value derives from the operator's pick first; only fall back
  // to the persisted policy or "direct" if they haven't changed it yet.
  const [override, setOverride] = useState<RoutingTarget | null>(null);
  const target = override ?? policyToTarget(current?.policy ?? "direct");

  async function handleContinue() {
    await setDefault.mutateAsync(targetToPolicy(target));
    await advance.mutateAsync({ to_step: "completed" });
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold text-ink">Default routing</h2>
        <p className="text-sm text-muted-foreground">
          Pick how new devices route by default. You can override per-device later from the Devices
          page.
        </p>
      </div>

      <div className="flex flex-col gap-2">
        <RoutingSelector value={target} onChange={setOverride} tunnels={tunnelList} isAdmin />
        {tunnelList.length === 0 && (
          <p className="text-xs text-muted-foreground">
            No tunnels configured — defaulting to direct routing. You can change this from Settings
            whenever you add a tunnel.
          </p>
        )}
      </div>

      <Button
        onClick={handleContinue}
        disabled={setDefault.isPending || advance.isPending}
        className="h-12 w-full"
      >
        {setDefault.isPending || advance.isPending ? "Saving…" : "Continue"}
      </Button>
    </div>
  );
}
