import { useState } from "react";
import { Button } from "@wardnet/web";
import { Heading, Text } from "@wardnet/web";
import { RoutingSelector } from "@wardnet/web";
import { useDefaultPolicy, useSetDefaultPolicy } from "@wardnet/web";
import { WizardFooter } from "@/pages/setup/WizardFooter";
import { useWizardNav } from "@/pages/setup/useWizardNav";
import { useTunnels } from "@wardnet/web";
import type { RoutingTarget } from "@wardnet/js";

/**
 * Policy step — pick the global default routing policy.
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

export default function StepPolicy() {
  const nav = useWizardNav("policy");
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
    nav.goNext();
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <Heading level={2} size="3xl" className="text-ink">
          Default routing
        </Heading>
        <Text as="p" size="sm" className="mt-1 text-ink-3">
          Pick how new devices route by default. You can override per-device
          later from the Devices page.
        </Text>
      </div>

      <div className="flex flex-col gap-2">
        <RoutingSelector
          value={target}
          onChange={setOverride}
          tunnels={tunnelList}
          isAdmin
        />
        {tunnelList.length === 0 && (
          <Text as="p" size="xs" className="text-ink-3">
            No tunnels configured — defaulting to direct routing. You can change
            this from Settings whenever you add a tunnel.
          </Text>
        )}
      </div>

      <WizardFooter>
        <Button
          onClick={handleContinue}
          disabled={setDefault.isPending || nav.isPending}
          data-testid="setup-policy-continue"
          className="w-full"
        >
          {setDefault.isPending || nav.isPending ? "Saving…" : "Continue"}
        </Button>
      </WizardFooter>
    </div>
  );
}
