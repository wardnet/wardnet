import { useMemo, useState } from "react";
import { Button } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { FormActions } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { Ipv4Input } from "@wardnet/web";
import { isPrivateIpv4 } from "@wardnet/web";
import { isCompleteIpv4, ipv4ToInt } from "@wardnet/js";
import { ApiErrorAlert } from "@wardnet/web";
import { useUpdateDhcpConfig, usePreviewDhcpConfig } from "@wardnet/web";
import { useDnsConfig } from "@wardnet/web";
import type { DhcpConfig, DhcpLease } from "@wardnet/js";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";

function formatDuration(secs: number): string {
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h`;
  return `${Math.floor(secs / 86400)}d`;
}

/** Human-readable warning listing the devices a pool change would strand. */
function affectedDescription(leases: DhcpLease[]): string {
  const shown = leases
    .slice(0, 5)
    .map((l) => `${l.hostname || l.mac_address} (${l.ip_address})`);
  const more = leases.length > 5 ? `, and ${leases.length - 5} more` : "";
  const count = leases.length;
  return (
    `${count} device${count === 1 ? "" : "s"} currently hold ` +
    `out-of-range leases and will reconnect within ~10 minutes: ` +
    `${shown.join(", ")}${more}.`
  );
}

interface DhcpConfigCardProps {
  config: DhcpConfig;
}

/** Card displaying the DHCP pool configuration with inline edit-mode.
 *  When the Wardnet DNS server is enabled, the upstream-DNS field
 *  collapses: the daemon will advertise Wardnet's own IP to clients
 *  regardless of what's saved here, so we hide the field in edit mode
 *  and show "Wardnet DNS" in the read view to match reality. */
export function DhcpConfigCard({ config }: DhcpConfigCardProps) {
  const updateConfig = useUpdateDhcpConfig();
  const previewConfig = usePreviewDhcpConfig();
  const { data: dnsConfigData } = useDnsConfig();
  const dnsEnabled = dnsConfigData?.config.enabled ?? false;

  const [editing, setEditing] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [affected, setAffected] = useState<DhcpLease[]>([]);
  const [poolStart, setPoolStart] = useState(config.pool_start);
  const [poolEnd, setPoolEnd] = useState(config.pool_end);
  const [subnetMask, setSubnetMask] = useState(config.subnet_mask);
  const [leaseDuration, setLeaseDuration] = useState(
    String(config.lease_duration_secs),
  );
  const [routerIp, setRouterIp] = useState(config.router_ip ?? "");
  const [upstreamDns, setUpstreamDns] = useState(
    config.upstream_dns.join(", "),
  );

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

  // Client-side guard for the pool editor. `Ipv4Input` already clamps
  // octets to 0–255, so we only catch incomplete addresses and a pool
  // range whose end precedes its start — the same rules the daemon
  // enforces, surfaced before the round-trip. Optional fields (router)
  // are only checked when the user typed something.
  const validationError = useMemo<string | null>(() => {
    if (!editing) return null;
    if (!isCompleteIpv4(poolStart))
      return "Enter a complete pool start address.";
    if (!isCompleteIpv4(poolEnd)) return "Enter a complete pool end address.";
    if (!isCompleteIpv4(subnetMask)) return "Enter a complete subnet mask.";
    if (routerIp !== "" && !isCompleteIpv4(routerIp))
      return "Enter a complete fallback router address.";
    if (ipv4ToInt(poolEnd) < ipv4ToInt(poolStart))
      return "Pool end must be at or after pool start.";
    // LAN addressing must be private (RFC 1918) — a public range would
    // blackhole real internet hosts.
    const privateHint = "a private range (10.x, 172.16-31.x, or 192.168.x)";
    if (!isPrivateIpv4(poolStart)) return `Pool start must be ${privateHint}.`;
    if (!isPrivateIpv4(poolEnd)) return `Pool end must be ${privateHint}.`;
    if (routerIp !== "" && !isPrivateIpv4(routerIp))
      return `Fallback router must be ${privateHint}.`;
    return null;
  }, [editing, poolStart, poolEnd, subnetMask, routerIp]);

  async function doSave() {
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

  async function handleSave() {
    if (validationError) return;

    // Only a pool-range change can strand existing leases, so dry-run the new
    // range and warn before saving when devices would be forced to reconnect
    // (issue #227). Preview is best-effort: if it fails, fall through to save.
    const poolChanged =
      poolStart !== config.pool_start || poolEnd !== config.pool_end;
    if (poolChanged) {
      try {
        const res = await previewConfig.mutateAsync({
          pool_start: poolStart,
          pool_end: poolEnd,
        });
        if (res.affected.length > 0) {
          setAffected(res.affected);
          setConfirmOpen(true);
          return;
        }
      } catch {
        // Ignore — the warning is a nicety, not a gate on saving.
      }
    }

    await doSave();
  }

  function confirmSave() {
    setConfirmOpen(false);
    void doSave();
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm font-medium text-ink-3">
          Configuration
        </CardTitle>
        {!editing && (
          <CardAction>
            <Button
              variant="outline"
              size="sm"
              onClick={startEdit}
              data-testid="dhcp-config-edit"
            >
              Edit
            </Button>
          </CardAction>
        )}
      </CardHeader>

      {editing ? (
        <>
          <CardContent className="flex flex-col gap-5">
            <div className="flex gap-3">
              <Field
                label="Pool start"
                htmlFor="dhcp-pool-start"
                className="flex-1"
              >
                <Ipv4Input
                  id="dhcp-pool-start"
                  data-testid="dhcp-pool-start"
                  value={poolStart}
                  onChange={setPoolStart}
                  placeholder="192.168.1.100"
                />
              </Field>
              <Field
                label="Pool end"
                htmlFor="dhcp-pool-end"
                className="flex-1"
              >
                <Ipv4Input
                  id="dhcp-pool-end"
                  data-testid="dhcp-pool-end"
                  value={poolEnd}
                  onChange={setPoolEnd}
                  placeholder="192.168.1.200"
                />
              </Field>
            </div>

            <Field label="Subnet mask" htmlFor="dhcp-subnet">
              <Ipv4Input
                id="dhcp-subnet"
                data-testid="dhcp-subnet"
                value={subnetMask}
                onChange={setSubnetMask}
                placeholder="255.255.255.0"
              />
            </Field>

            <div className="flex gap-3">
              <Field
                label="Lease duration (seconds)"
                htmlFor="dhcp-lease"
                className="flex-1"
              >
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
                help="Your real router's IP. Used as a secondary gateway so devices fall back if Wardnet is unavailable."
                className="flex-1"
              >
                <Ipv4Input
                  id="dhcp-router"
                  data-testid="dhcp-router"
                  value={routerIp}
                  onChange={setRouterIp}
                  placeholder="10.232.1.1"
                />
              </Field>
            </div>

            {!dnsEnabled && (
              <Field
                label="Upstream DNS (comma-separated)"
                htmlFor="dhcp-dns"
                help="DNS servers advertised to clients."
              >
                <Input
                  id="dhcp-dns"
                  value={upstreamDns}
                  onChange={(e) => setUpstreamDns(e.target.value)}
                  placeholder="1.1.1.1, 8.8.8.8"
                />
              </Field>
            )}

            {validationError && (
              <Text
                as="p"
                size="sm"
                className="text-danger-soft-ink"
                data-testid="dhcp-config-validation"
              >
                {validationError}
              </Text>
            )}

            {updateConfig.isError && (
              <ApiErrorAlert
                error={updateConfig.error}
                fallback="Failed to update configuration"
              />
            )}
          </CardContent>
          <FormActions
            secondaryLabel="Cancel"
            secondaryProps={{
              onClick: cancelEdit,
              disabled: updateConfig.isPending || previewConfig.isPending,
              "data-testid": "dhcp-config-cancel",
            }}
            primaryLabel={
              previewConfig.isPending
                ? "Checking…"
                : updateConfig.isPending
                  ? "Saving…"
                  : "Save"
            }
            primaryProps={{
              onClick: handleSave,
              disabled:
                updateConfig.isPending ||
                previewConfig.isPending ||
                validationError !== null,
              "data-testid": "dhcp-config-save",
            }}
          />
          <ConfirmDialog
            open={confirmOpen}
            onOpenChange={setConfirmOpen}
            title="Devices will reconnect"
            description={affectedDescription(affected)}
            confirmLabel="Save and revoke"
            onConfirm={confirmSave}
          />
        </>
      ) : (
        <CardContent>
          <dl className="grid grid-cols-1 gap-x-8 gap-y-3 text-sm sm:grid-cols-2 lg:grid-cols-3">
            <div>
              <dt className="text-ink-3">Gateway IP</dt>
              <Text as="dd" size="xs" className="font-mono">
                {config.gateway_ip}
              </Text>
            </div>
            <div>
              <dt className="text-ink-3">Pool range</dt>
              <Text
                as="dd"
                size="xs"
                className="font-mono"
                data-testid="dhcp-config-pool-range"
              >
                {config.pool_start} &ndash; {config.pool_end}
              </Text>
            </div>
            <div>
              <dt className="text-ink-3">Subnet</dt>
              <Text as="dd" size="xs" className="font-mono">
                {config.subnet_mask}
              </Text>
            </div>
            <div>
              <dt className="text-ink-3">Lease duration</dt>
              <Text as="dd" weight="medium">
                {formatDuration(config.lease_duration_secs)}
              </Text>
            </div>
            <div>
              <dt className="text-ink-3">Fallback router</dt>
              <Text as="dd" size="xs" className="font-mono">
                {config.router_ip ?? "-"}
              </Text>
            </div>
            <div>
              <dt className="text-ink-3">Upstream DNS</dt>
              <dd className={dnsEnabled ? "font-medium" : "font-mono text-xs"}>
                {dnsEnabled
                  ? "Wardnet DNS"
                  : config.upstream_dns.join(", ") || "-"}
              </dd>
            </div>
          </dl>
        </CardContent>
      )}
    </Card>
  );
}
