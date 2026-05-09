import { useState } from "react";
import { Button } from "@wardnet/forge-web/button";
import { CreateTunnelInline } from "@/components/features/CreateTunnelInline";
import { useAdvanceWizard } from "@/hooks/useSetup";
import { useTunnels } from "@/hooks/useTunnels";

/**
 * Step 5 — first VPN tunnel (optional).
 *
 * Inlines `CreateTunnelInline` (the same component the Tunnels page
 * uses) directly inside the auth card so the operator can import a
 * tunnel without leaving the wizard — `SetupGuard` would bounce them
 * straight back here if they tried to navigate to `/tunnels` before
 * finishing setup.
 *
 * The Skip button always advances to step 6; step 6's picker
 * defaults to "direct" when no tunnels exist.
 */
export default function Step5Tunnel() {
  const advance = useAdvanceWizard();
  const { data: tunnels } = useTunnels();
  const [adding, setAdding] = useState(false);
  const tunnelCount = tunnels?.tunnels.length ?? 0;
  const hasTunnel = tunnelCount > 0;

  if (adding) {
    return <CreateTunnelInline onClose={() => setAdding(false)} embedded />;
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold text-foreground">First VPN tunnel</h2>
        <p className="text-sm text-muted-foreground">
          Optional — connect a VPN provider so opted-in devices can route through it. You can add
          more from the Tunnels page once setup is complete.
        </p>
      </div>

      {hasTunnel ? (
        <div className="rounded-lg border border-border bg-muted/30 p-4 text-sm text-muted-foreground">
          {tunnelCount} tunnel{tunnelCount === 1 ? "" : "s"} configured. Continue to pick a default
          routing policy.
        </div>
      ) : (
        <div className="rounded-lg border border-border bg-muted/30 p-4 text-sm text-muted-foreground">
          No tunnels yet. Add one now or skip — you can change the default routing policy from
          Settings later.
        </div>
      )}

      <div className="flex flex-col gap-2">
        <Button variant="outline" onClick={() => setAdding(true)} className="h-12 w-full">
          {hasTunnel ? "Add another tunnel" : "Add tunnel"}
        </Button>
        <Button
          onClick={() => advance.mutate({ to_step: "policy" })}
          disabled={advance.isPending}
          className="h-12 w-full"
        >
          {advance.isPending ? "Saving…" : hasTunnel ? "Continue" : "Skip for now"}
        </Button>
      </div>
    </div>
  );
}
