import type { ReactNode } from "react";
import type { WizardStep } from "@wardnet/js";
import { Button, Heading, Text } from "@wardnet/web";
import {
  useDdnsStatus,
  useDefaultPolicy,
  useDnsFilterConfig,
  useDnsFilterProfiles,
  useMe,
  useNetworkStatus,
  useSetupStatus,
  useTunnels,
} from "@wardnet/web";
import { WizardFooter } from "@/pages/setup/WizardFooter";
import { useWizardNav } from "@/pages/setup/useWizardNav";

/**
 * Review step — read-only summary of everything configured so far.
 *
 * Every wizard step already committed its changes when it ran, so this
 * is purely informational: each row reads the daemon's current state
 * through the same query hooks the rest of the app uses, and the Edit
 * links rewind the wizard to the row's step (the daemon allows rewinds
 * down to the `network` floor — which is why the Admin row has no Edit).
 * "Finish setup" just advances to `completed`; nothing is applied here.
 */
export default function StepReview() {
  const nav = useWizardNav("review");
  const { data: me } = useMe();
  const { data: network } = useNetworkStatus();
  const { data: setupStatus } = useSetupStatus();
  const { data: dnsConfig } = useDnsFilterConfig();
  const { data: dnsProfiles } = useDnsFilterProfiles();
  const { data: tunnels } = useTunnels();
  const { data: defaultPolicy } = useDefaultPolicy();
  const { data: ddns } = useDdnsStatus();

  const tunnelList = tunnels?.tunnels ?? [];

  const dhcpMode =
    setupStatus?.wizard_mode === "locked_router"
      ? "Locked router"
      : setupStatus?.wizard_mode === "primary"
        ? "Primary"
        : "-";

  const defaultProfileNames = (dnsConfig?.config.default_profile_ids ?? [])
    .map((id) => (dnsProfiles?.profiles ?? []).find((p) => p.id === id)?.name)
    .filter((name): name is string => Boolean(name));
  const dnsSummary =
    dnsConfig?.config.enabled === false
      ? "Disabled"
      : defaultProfileNames.length > 0
        ? defaultProfileNames.join(", ")
        : "No profiles";

  const tunnelSummary =
    tunnelList.length === 0
      ? "None"
      : tunnelList.length === 1
        ? tunnelList[0].label
        : `${tunnelList.length} tunnels`;

  const policy = defaultPolicy?.policy;
  const policySummary =
    !policy || policy === "direct"
      ? "Direct (no VPN)"
      : (tunnelList.find((t) => t.id === policy)?.label ?? "Tunnel");

  const rows: Array<{
    key: string;
    label: string;
    value: ReactNode;
    editStep?: WizardStep;
  }> = [
    { key: "admin", label: "Admin user", value: me?.username ?? "-" },
    {
      key: "lan-ip",
      label: "LAN IP",
      value: <span className="mono">{network?.ip ?? "-"}</span>,
      editStep: "network",
    },
    { key: "dhcp", label: "DHCP mode", value: dhcpMode, editStep: "dhcp" },
    {
      key: "router-mac",
      label: "Router MAC",
      value: network?.router_mac ? (
        <span className="mono">{network.router_mac}</span>
      ) : (
        "Skipped"
      ),
      editStep: "router_mac",
    },
    { key: "dns", label: "DNS filtering", value: dnsSummary, editStep: "dns" },
    {
      key: "tunnel",
      label: "VPN tunnel",
      value: tunnelSummary,
      editStep: "tunnel",
    },
    {
      key: "policy",
      label: "Default routing",
      value: policySummary,
      editStep: "policy",
    },
    {
      key: "remote-access",
      label: "Remote access",
      value: ddns?.fqdn ?? "Skipped",
      editStep: "remote_access",
    },
  ];

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <Heading level={2} size="3xl" className="text-ink">
          Review &amp; finish
        </Heading>
        <Text as="p" size="sm" className="mt-1 text-ink-3">
          Everything below is already saved - confirm it looks right, or jump
          back to a step to change it.
        </Text>
      </div>

      <div className="overflow-hidden rounded-lg border border-line">
        {rows.map((row, i) => (
          <div
            key={row.key}
            className={
              i < rows.length - 1
                ? "flex items-center justify-between gap-3 border-b border-line px-4 py-3"
                : "flex items-center justify-between gap-3 px-4 py-3"
            }
          >
            <Text as="span" size="sm" className="shrink-0 text-ink-3">
              {row.label}
            </Text>
            <div className="flex min-w-0 items-center gap-3">
              <Text
                as="span"
                size="sm"
                weight="medium"
                className="truncate text-ink"
              >
                {row.value}
              </Text>
              {row.editStep && (
                <button
                  type="button"
                  onClick={() => nav.goTo(row.editStep as WizardStep)}
                  disabled={nav.isPending}
                  data-testid={`setup-review-edit-${row.key}`}
                  className="text-accent-soft-ink hover:underline"
                >
                  <Text as="span" size="xs" weight="medium">
                    Edit
                  </Text>
                </button>
              )}
            </div>
          </div>
        ))}
      </div>

      {nav.isError && (
        <Text as="p" size="sm" className="text-danger">
          Couldn't save progress - try again.
        </Text>
      )}

      <WizardFooter>
        <Button
          onClick={() => nav.goNext()}
          disabled={nav.isPending}
          data-testid="setup-review-finish"
          className="w-full"
        >
          {nav.isPending ? "Finishing…" : "Finish setup"}
        </Button>
      </WizardFooter>
    </div>
  );
}
