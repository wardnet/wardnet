import { useState } from "react";
import { Button } from "@wardnet/forge-web/button";
import {
  Card,
  CardAction,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@wardnet/forge-web/card";
import { Field } from "@wardnet/forge-web/field";
import { Input } from "@wardnet/forge-web/input";
import { Ipv4Input } from "@/components/core/ui/ipv4-input";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { useUpdateDhcpConfig } from "@/hooks/useDhcp";
import type { DhcpConfig } from "@wardnet/js";

function formatDuration(secs: number): string {
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h`;
  return `${Math.floor(secs / 86400)}d`;
}

interface DhcpConfigCardProps {
  config: DhcpConfig;
}

/** Card displaying the DHCP pool configuration with inline edit-mode. */
export function DhcpConfigCard({ config }: DhcpConfigCardProps) {
  const updateConfig = useUpdateDhcpConfig();

  const [editing, setEditing] = useState(false);
  const [poolStart, setPoolStart] = useState(config.pool_start);
  const [poolEnd, setPoolEnd] = useState(config.pool_end);
  const [subnetMask, setSubnetMask] = useState(config.subnet_mask);
  const [leaseDuration, setLeaseDuration] = useState(String(config.lease_duration_secs));
  const [routerIp, setRouterIp] = useState(config.router_ip ?? "");
  const [upstreamDns, setUpstreamDns] = useState(config.upstream_dns.join(", "));

  function startEdit() {
    setPoolStart(config.pool_start);
    setPoolEnd(config.pool_end);
    setSubnetMask(config.subnet_mask);
    setLeaseDuration(String(config.lease_duration_secs));
    setRouterIp(config.router_ip ?? "");
    setUpstreamDns(config.upstream_dns.join(", "));
    updateConfig.reset();
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    updateConfig.reset();
  }

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
    setEditing(false);
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm font-medium text-ink-3">Configuration</CardTitle>
        {!editing && (
          <CardAction>
            <Button variant="outline" size="sm" onClick={startEdit}>
              Edit
            </Button>
          </CardAction>
        )}
      </CardHeader>

      {editing ? (
        <>
          <CardContent className="flex flex-col gap-5">
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
          </CardContent>
          <CardFooter className="justify-end gap-2">
            <Button variant="ghost" onClick={cancelEdit} disabled={updateConfig.isPending}>
              Cancel
            </Button>
            <Button onClick={handleSave} disabled={updateConfig.isPending}>
              {updateConfig.isPending ? "Saving…" : "Save"}
            </Button>
          </CardFooter>
        </>
      ) : (
        <CardContent>
          <dl className="grid grid-cols-1 gap-x-8 gap-y-3 text-sm sm:grid-cols-2 lg:grid-cols-3">
            <div>
              <dt className="text-ink-3">Gateway IP</dt>
              <dd className="font-mono text-xs">{config.gateway_ip}</dd>
            </div>
            <div>
              <dt className="text-ink-3">Pool range</dt>
              <dd className="font-mono text-xs">
                {config.pool_start} &ndash; {config.pool_end}
              </dd>
            </div>
            <div>
              <dt className="text-ink-3">Subnet</dt>
              <dd className="font-mono text-xs">{config.subnet_mask}</dd>
            </div>
            <div>
              <dt className="text-ink-3">Lease duration</dt>
              <dd className="font-medium">{formatDuration(config.lease_duration_secs)}</dd>
            </div>
            <div>
              <dt className="text-ink-3">Fallback router</dt>
              <dd className="font-mono text-xs">{config.router_ip ?? "—"}</dd>
            </div>
            <div>
              <dt className="text-ink-3">Upstream DNS</dt>
              <dd className="font-mono text-xs">{config.upstream_dns.join(", ") || "—"}</dd>
            </div>
          </dl>
        </CardContent>
      )}
    </Card>
  );
}
