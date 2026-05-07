import { Button } from "@/components/core/ui/button";
import { useAdvanceWizard } from "@/hooks/useSetup";

/**
 * Step 2 — confirm the OS network state.
 *
 * For the v1 wizard this is a read-only acknowledgement: the LAN
 * IP was set up at install time via `install.sh --static-ip`. A
 * later commit adds GET /api/network/status so this step can show
 * the live IP and a remediation panel when the IP is still
 * DHCP-derived.
 */
export default function Step2Network() {
  const advance = useAdvanceWizard();

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold text-foreground">Confirm network</h2>
        <p className="text-sm text-muted-foreground">
          Wardnet should have a stable LAN IP so opted-in devices keep pointing at it across
          reboots.
        </p>
      </div>
      <div className="rounded-lg border border-border bg-muted/30 p-4 text-sm text-muted-foreground">
        <p>
          If you supplied <code>--static-ip</code> when running <code>install.sh</code>, your LAN
          address is already pinned. Otherwise this host is using whatever IP your router handed
          out, and you may want to re-run <code>install.sh</code> with a fixed CIDR (e.g.{" "}
          <code>--static-ip 192.168.1.2/24</code>).
        </p>
      </div>
      <Button
        onClick={() => advance.mutate({ to_step: "dhcp" })}
        disabled={advance.isPending}
        className="h-12 w-full"
      >
        {advance.isPending ? "Saving…" : "Continue"}
      </Button>
    </div>
  );
}
