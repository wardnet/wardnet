import { AlertTriangleIcon } from "lucide-react";
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
        <h2 className="h-title">Confirm network</h2>
        <p className="h-sub">
          Wardnet should have a stable LAN IP so opted-in devices keep pointing at it across
          reboots.
        </p>
      </div>

      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 rounded-lg border border-line bg-sunken p-4 text-sm">
        <dt className="text-ink-3">Interface</dt>
        <dd className="mono">{isLoading ? "…" : isError ? "—" : (data?.interface ?? "—")}</dd>
        <dt className="text-ink-3">IP address</dt>
        <dd className="mono">{isLoading ? "…" : isError ? "—" : (data?.ip ?? "—")}</dd>
        <dt className="text-ink-3">Gateway</dt>
        <dd className="mono">
          {isLoading ? "…" : isError ? "—" : (data?.gateway ?? "not detected")}
        </dd>
        <dt className="text-ink-3">Source</dt>
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
        <div
          role="status"
          className="flex items-start gap-2 rounded-md border border-warn-soft bg-warn-soft px-3 py-2.5 text-warn-soft-ink"
        >
          <AlertTriangleIcon className="mt-0.5 size-4 shrink-0" />
          <div className="flex flex-col gap-1 text-sm">
            <p className="font-medium">Your IP isn't pinned</p>
            <p>
              The router is currently leasing this address — it may change on the next reboot.
              Re-run <code>install.sh</code> with{" "}
              <code>--static-ip {data?.ip ?? "&lt;cidr&gt;"}/24</code> (or another IP from your
              subnet) to write <code>/etc/dhcpcd.conf.d/wardnet.conf</code> and pin it. You can
              continue without this, but devices may need reconfiguring later.
            </p>
          </div>
        </div>
      )}

      <Button
        onClick={() => advance.mutate({ to_step: "dhcp" })}
        disabled={advance.isPending}
        className="w-full"
      >
        {advance.isPending ? "Saving…" : "Continue"}
      </Button>
    </div>
  );
}
