import { Link } from "react-router";
import { Button } from "@/components/core/ui/button";
import { useAdvanceWizard } from "@/hooks/useSetup";
import { useTunnels } from "@/hooks/useTunnels";

/**
 * Step 5 — first VPN tunnel (optional).
 *
 * The full UX (per the design plan) is to wrap the BYO `.conf` import
 * sheet from `Tunnels.tsx` and the NordVPN provider sheet from
 * `TunnelDetail.tsx` directly inside the wizard so the operator can
 * import a tunnel without leaving setup.
 *
 * For v1 we punt to those existing pages: a "Set up tunnel" link opens
 * `/tunnels` in the same tab, and a "Skip" button advances to step 6.
 * If a tunnel already exists by the time the operator returns, the
 * wizard treats step 5 as done and presents the policy picker on
 * step 6.
 */
export default function Step5Tunnel() {
  const advance = useAdvanceWizard();
  const { data: tunnels } = useTunnels();
  const hasTunnel = (tunnels?.tunnels.length ?? 0) > 0;

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold text-foreground">First VPN tunnel</h2>
        <p className="text-sm text-muted-foreground">
          Optional — connect a VPN provider so opted-in devices can route through it. You can do
          this later from the Tunnels page.
        </p>
      </div>

      {hasTunnel ? (
        <div className="rounded-lg border border-border bg-muted/30 p-4 text-sm text-muted-foreground">
          You already have {tunnels!.tunnels.length} tunnel
          {tunnels!.tunnels.length === 1 ? "" : "s"} configured. Continue to pick a default routing
          policy.
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          <Button asChild variant="outline" className="h-12 w-full">
            <Link to="/tunnels">Open the Tunnels page to add one</Link>
          </Button>
          <p className="text-xs text-muted-foreground">
            Use the Tunnels page to import a WireGuard config or run the NordVPN provider flow, then
            come back here.
          </p>
        </div>
      )}

      <Button
        onClick={() => advance.mutate({ to_step: "policy" })}
        disabled={advance.isPending}
        className="h-12 w-full"
      >
        {advance.isPending ? "Saving…" : hasTunnel ? "Continue" : "Skip for now"}
      </Button>
    </div>
  );
}
