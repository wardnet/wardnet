import { Button } from "@wardnet/forge-web/button";
import { useAdvanceWizard } from "@/hooks/useSetup";
import { useNetworkStatus } from "@/hooks/useNetwork";

/**
 * Step 2 — confirm the OS network state.
 *
 * Reads `GET /api/network/status` to show the LAN interface, IP, and
 * gateway as currently seen by the kernel. Surfaces a remediation
 * panel pointing back at `install.sh --static-ip` whenever the IP is
 * still DHCP-derived; otherwise just confirms the values.
 */
export default function Step2Network() {
  const advance = useAdvanceWizard();
  const { data, isLoading, isError } = useNetworkStatus();

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold text-ink">Confirm network</h2>
        <p className="text-sm text-muted-foreground">
          Wardnet should have a stable LAN IP so opted-in devices keep pointing at it across
          reboots.
        </p>
      </div>

      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 rounded-lg border border-border bg-muted/30 p-4 text-sm">
        <dt className="text-muted-foreground">Interface</dt>
        <dd className="font-mono">{isLoading ? "…" : isError ? "—" : (data?.interface ?? "—")}</dd>
        <dt className="text-muted-foreground">IP address</dt>
        <dd className="font-mono">{isLoading ? "…" : isError ? "—" : (data?.ip ?? "—")}</dd>
        <dt className="text-muted-foreground">Gateway</dt>
        <dd className="font-mono">
          {isLoading ? "…" : isError ? "—" : (data?.gateway ?? "not detected")}
        </dd>
        <dt className="text-muted-foreground">Source</dt>
        <dd>
          {isLoading
            ? "…"
            : isError
              ? "—"
              : data?.dhcp_source === "static"
                ? "Static (install.sh)"
                : data?.dhcp_source === "dhcp"
                  ? "DHCP (router-assigned)"
                  : "Unknown"}
        </dd>
      </dl>

      {data?.dhcp_source !== "static" && (
        <div className="rounded-lg border border-warn/40 bg-warn/10 p-4 text-sm">
          <p className="font-medium text-ink">Your IP isn't pinned</p>
          <p className="mt-1 text-muted-foreground">
            The router is currently leasing this address — it may change on the next reboot. Re-run{" "}
            <code>install.sh</code> with <code>--static-ip {data?.ip ?? "&lt;cidr&gt;"}/24</code>{" "}
            (or another IP from your subnet) to write <code>/etc/dhcpcd.conf.d/wardnet.conf</code>{" "}
            and pin it. You can continue without this, but devices may need reconfiguring later.
          </p>
        </div>
      )}

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
