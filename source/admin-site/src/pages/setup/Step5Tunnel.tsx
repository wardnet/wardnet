import { useState } from "react";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { CreateTunnelInline } from "@/components/features/CreateTunnelInline";
import { useAdvanceWizard } from "@wardnet/web";
import { useTunnels } from "@wardnet/web";

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
        <h2 className="h-title">First VPN tunnel</h2>
        <p className="h-sub">
          Optional — connect a VPN provider so opted-in devices can route
          through it. You can add more from the Tunnels page once setup is
          complete.
        </p>
      </div>

      {hasTunnel ? (
        <Text
          as="div"
          size="sm"
          className="rounded-md border border-line bg-sunken p-4 text-ink-3"
        >
          {tunnelCount} tunnel{tunnelCount === 1 ? "" : "s"} configured.
          Continue to pick a default routing policy.
        </Text>
      ) : (
        <Text
          as="div"
          size="sm"
          className="rounded-md border border-line bg-sunken p-4 text-ink-3"
        >
          No tunnels yet. Add one now or skip — you can change the default
          routing policy from Settings later.
        </Text>
      )}

      <div className="flex flex-col gap-2">
        <Button
          variant="outline"
          onClick={() => setAdding(true)}
          className="w-full"
        >
          {hasTunnel ? "Add another tunnel" : "Add tunnel"}
        </Button>
        <Button
          onClick={() => advance.mutate({ to_step: "policy" })}
          disabled={advance.isPending}
          className="w-full"
        >
          {advance.isPending
            ? "Saving…"
            : hasTunnel
              ? "Continue"
              : "Skip for now"}
        </Button>
      </div>
    </div>
  );
}
