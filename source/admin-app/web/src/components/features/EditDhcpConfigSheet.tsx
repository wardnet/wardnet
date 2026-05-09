import { useState } from "react";
import { Button } from "@wardnet/forge-web/button";
import { Field } from "@wardnet/forge-web/field";
import { Input } from "@wardnet/forge-web/input";
import { Ipv4Input } from "@/components/core/ui/ipv4-input";
import { Sheet, SheetContent, SheetTitle } from "@/components/core/ui/sheet";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { useUpdateDhcpConfig } from "@/hooks/useDhcp";
import type { DhcpConfig } from "@wardnet/js";

interface EditDhcpConfigSheetProps {
  config: DhcpConfig;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Sheet form for editing DHCP pool configuration. */
export function EditDhcpConfigSheet({ config, open, onOpenChange }: EditDhcpConfigSheetProps) {
  const updateConfig = useUpdateDhcpConfig();

  const [poolStart, setPoolStart] = useState(config.pool_start);
  const [poolEnd, setPoolEnd] = useState(config.pool_end);
  const [subnetMask, setSubnetMask] = useState(config.subnet_mask);
  const [leaseDuration, setLeaseDuration] = useState(String(config.lease_duration_secs));
  const [routerIp, setRouterIp] = useState(config.router_ip ?? "");
  const [upstreamDns, setUpstreamDns] = useState(config.upstream_dns.join(", "));

  async function handleSave() {
    await updateConfig.mutateAsync({
      pool_start: poolStart,
      pool_end: poolEnd,
      subnet_mask: subnetMask,
      lease_duration_secs: Number(leaseDuration),
      upstream_dns: upstreamDns
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean),
      router_ip: routerIp || undefined,
    });
    onOpenChange(false);
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full overflow-y-auto p-6">
        <SheetTitle>Edit DHCP configuration</SheetTitle>
        <div className="mt-6 flex flex-col gap-5">
          <div className="flex gap-3">
            <Field label="Pool start" htmlFor="dhcp-pool-start" className="flex-1">
              <Ipv4Input
                id="dhcp-pool-start"
                value={poolStart}
                onChange={setPoolStart}
                placeholder="192.168.1.100"
              />
            </Field>
            <Field label="Pool end" htmlFor="dhcp-pool-end" className="flex-1">
              <Ipv4Input
                id="dhcp-pool-end"
                value={poolEnd}
                onChange={setPoolEnd}
                placeholder="192.168.1.200"
              />
            </Field>
          </div>

          <Field label="Subnet mask" htmlFor="dhcp-subnet">
            <Ipv4Input
              id="dhcp-subnet"
              value={subnetMask}
              onChange={setSubnetMask}
              placeholder="255.255.255.0"
            />
          </Field>

          <Field label="Lease duration (seconds)" htmlFor="dhcp-lease">
            <Input
              id="dhcp-lease"
              type="number"
              value={leaseDuration}
              onChange={(e) => setLeaseDuration(e.target.value)}
              placeholder="86400"
            />
          </Field>

          <Field
            label="Fallback router"
            htmlFor="dhcp-router"
            help="Your real router's IP. Included as secondary gateway in DHCP so devices fall back if the wardnet server is unavailable."
          >
            <Ipv4Input
              id="dhcp-router"
              value={routerIp}
              onChange={setRouterIp}
              placeholder="10.232.1.1"
            />
          </Field>

          <Field
            label="Upstream DNS (comma-separated)"
            htmlFor="dhcp-dns"
            help="DNS servers advertised to clients. Will be replaced by Wardnet's built-in DNS once enabled."
          >
            <Input
              id="dhcp-dns"
              value={upstreamDns}
              onChange={(e) => setUpstreamDns(e.target.value)}
              placeholder="1.1.1.1, 8.8.8.8"
            />
          </Field>

          {updateConfig.isError && (
            <ApiErrorAlert error={updateConfig.error} fallback="Failed to update configuration" />
          )}

          <Button onClick={handleSave} disabled={updateConfig.isPending} className="w-full">
            {updateConfig.isPending ? "Saving…" : "Save configuration"}
          </Button>
        </div>
      </SheetContent>
    </Sheet>
  );
}
