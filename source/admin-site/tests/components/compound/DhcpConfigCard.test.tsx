import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DhcpConfig } from "@wardnet/js";
import { DhcpConfigCard } from "@/components/compound/DhcpConfigCard";
import { renderWithProviders } from "../../test-utils";

const updateMutateAsync = vi.fn();
const previewMutateAsync = vi.fn();
const updateReset = vi.fn();

function makeConfig(overrides: Partial<DhcpConfig> = {}): DhcpConfig {
  return {
    enabled: true,
    gateway_ip: "10.232.1.1",
    pool_start: "10.232.1.100",
    pool_end: "10.232.1.200",
    subnet_mask: "255.255.255.0",
    upstream_dns: ["1.1.1.1", "8.8.8.8"],
    lease_duration_secs: 86400,
    router_ip: "10.232.1.254",
    ...overrides,
  };
}

function cardProps({
  config = makeConfig(),
  dnsEnabled = false as boolean | undefined,
  update = {},
}: {
  config?: DhcpConfig;
  dnsEnabled?: boolean | undefined;
  update?: Partial<{
    isPending: boolean;
    isError: boolean;
    error: Error | null;
  }>;
} = {}) {
  return {
    config,
    dnsEnabled,
    updateConfig: {
      mutateAsync: updateMutateAsync,
      reset: updateReset,
      isPending: false,
      isError: false,
      error: null,
      ...update,
    },
    previewConfig: {
      mutateAsync: previewMutateAsync,
      reset: vi.fn(),
      isPending: false,
      isError: false,
      error: null,
    },
  };
}

beforeEach(() => {
  updateMutateAsync.mockReset().mockResolvedValue(undefined);
  previewMutateAsync.mockReset().mockResolvedValue({ affected: [] });
  updateReset.mockReset();
});

// biome-ignore lint/security/noSecrets: identifier-shaped string, not a credential — the entropy heuristic misfires on long CamelCase names
describe("DhcpConfigCard", () => {
  it("renders the read view with pool range and upstream DNS", () => {
    renderWithProviders(<DhcpConfigCard {...cardProps()} />);
    expect(screen.getByTestId("dhcp-config-pool-range")).toHaveTextContent(
      "10.232.1.100",
    );
    expect(screen.getByText("1d")).toBeInTheDocument();
    expect(screen.getByText("1.1.1.1, 8.8.8.8")).toBeInTheDocument();
  });

  it("formats sub-hour and sub-day lease durations", () => {
    const { rerender } = renderWithProviders(
      <DhcpConfigCard
        {...cardProps({ config: makeConfig({ lease_duration_secs: 1800 }) })}
      />,
    );
    expect(screen.getByText("30m")).toBeInTheDocument();
    rerender(
      <DhcpConfigCard
        {...cardProps({ config: makeConfig({ lease_duration_secs: 7200 }) })}
      />,
    );
    expect(screen.getByText("2h")).toBeInTheDocument();
  });

  it("shows 'Wardnet DNS' in the read view when the DNS server is enabled", () => {
    renderWithProviders(
      <DhcpConfigCard {...cardProps({ dnsEnabled: true })} />,
    );
    expect(screen.getByText("Wardnet DNS")).toBeInTheDocument();
  });

  // "Wardnet DNS" is what NEW leases get. DHCP cannot push it to a device
  // already holding a lease, and such a device resolves unfiltered without any
  // sign of it — so the read view must not imply the whole network is covered.
  // The warning rides an info icon's tooltip rather than sitting inline, so
  // assert on the accessible name: that is both what a screen reader announces
  // and the only copy a sighted user can surface by hovering.
  it("warns that already-leased devices keep their old DNS until they reconnect", () => {
    renderWithProviders(
      <DhcpConfigCard {...cardProps({ dnsEnabled: true })} />,
    );
    expect(screen.getByTestId("dhcp-dns-lease-note")).toHaveAccessibleName(
      /reconnect a device/i,
    );
  });

  it("omits the lease note when the DNS server is off", () => {
    renderWithProviders(
      <DhcpConfigCard {...cardProps({ dnsEnabled: false })} />,
    );
    expect(screen.queryByTestId("dhcp-dns-lease-note")).not.toBeInTheDocument();
  });

  it("shows a placeholder, not the upstream list, while the DNS query is unresolved", () => {
    // While the page's ["dns"] query is loading (or errored) the effective
    // client DNS is unknown — rendering the raw upstream list would claim
    // clients bypass the Pi when the daemon may actually be advertising it.
    renderWithProviders(
      <DhcpConfigCard {...cardProps()} dnsEnabled={undefined} />,
    );
    expect(screen.getByText("…")).toBeInTheDocument();
    expect(screen.queryByText(/1\.1\.1\.1/)).not.toBeInTheDocument();
    expect(screen.queryByText("Wardnet DNS")).not.toBeInTheDocument();
  });

  it("enters edit mode and hides the upstream DNS field when DNS is enabled", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DhcpConfigCard {...cardProps({ dnsEnabled: true })} />,
    );
    await user.click(screen.getByTestId("dhcp-config-edit"));
    expect(screen.getByTestId("dhcp-pool-start")).toBeInTheDocument();
    expect(screen.queryByLabelText(/Upstream DNS/)).not.toBeInTheDocument();
  });

  it("surfaces a validation error for an out-of-order pool range", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DhcpConfigCard
        {...cardProps({
          config: makeConfig({
            pool_start: "10.232.1.200",
            pool_end: "10.232.1.100",
          }),
        })}
      />,
    );
    await user.click(screen.getByTestId("dhcp-config-edit"));
    expect(screen.getByTestId("dhcp-config-validation")).toHaveTextContent(
      "Pool end must be at or after pool start.",
    );
    expect(screen.getByTestId("dhcp-config-save")).toBeDisabled();
  });

  it("rejects a non-private (public) pool", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DhcpConfigCard
        {...cardProps({
          config: makeConfig({
            pool_start: "8.8.8.100",
            pool_end: "8.8.8.200",
          }),
        })}
      />,
    );
    await user.click(screen.getByTestId("dhcp-config-edit"));
    expect(screen.getByTestId("dhcp-config-validation")).toHaveTextContent(
      /Pool start must be a private range/,
    );
    expect(screen.getByTestId("dhcp-config-save")).toBeDisabled();
  });

  it("cancels back to the read view", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DhcpConfigCard {...cardProps()} />);
    await user.click(screen.getByTestId("dhcp-config-edit"));
    await user.click(screen.getByTestId("dhcp-config-cancel"));
    expect(screen.queryByTestId("dhcp-pool-start")).not.toBeInTheDocument();
    expect(updateReset).toHaveBeenCalled();
  });

  it("saves directly when the pool range is unchanged", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DhcpConfigCard {...cardProps()} />);
    await user.click(screen.getByTestId("dhcp-config-edit"));
    await user.click(screen.getByTestId("dhcp-config-save"));
    expect(previewMutateAsync).not.toHaveBeenCalled();
    expect(updateMutateAsync).toHaveBeenCalledOnce();
  });

  it("warns before stranding leases when the pool range changes", async () => {
    const user = userEvent.setup();
    previewMutateAsync.mockResolvedValue({
      affected: [
        {
          id: "l1",
          mac_address: "AA:BB:CC:DD:EE:01",
          ip_address: "10.232.1.50",
          hostname: "laptop",
        },
      ],
    });
    renderWithProviders(<DhcpConfigCard {...cardProps()} />);
    await user.click(screen.getByTestId("dhcp-config-edit"));

    // Change the pool-start octets so the range differs from the config.
    const startInputs = screen
      .getByTestId("dhcp-pool-start")
      .querySelectorAll("input");
    await user.click(startInputs[0]);
    await user.paste("10.232.1.150");

    await user.click(screen.getByTestId("dhcp-config-save"));
    expect(previewMutateAsync).toHaveBeenCalled();
    const warning = await screen.findByText(/currently hold/);
    expect(warning).toBeInTheDocument();
    expect(warning).toHaveTextContent("laptop");

    await user.click(screen.getByText("Save and revoke"));
    expect(updateMutateAsync).toHaveBeenCalledOnce();
  });

  // The validation gate is what stands between a typo and a DHCP scope that
  // strands the network, so each rejection reason is worth pinning to the
  // field that triggers it.
  it("rejects an incomplete pool start", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DhcpConfigCard {...cardProps()} />);
    await user.click(screen.getByTestId("dhcp-config-edit"));
    const input = screen
      .getByTestId("dhcp-pool-start")
      .querySelectorAll("input")[0];
    await user.click(input);
    // biome-ignore lint/security/noSecrets: identifier-shaped string, not a credential — the entropy heuristic misfires on long CamelCase names
    await user.keyboard("{Control>}a{/Control}{Backspace}");
    expect(
      await screen.findByText("Enter a complete pool start address."),
    ).toBeInTheDocument();
    expect(screen.getByTestId("dhcp-config-save")).toBeDisabled();
  });

  // Clearing an octet rather than pasting a partial string: Ipv4Input's paste
  // handler only accepts a full dotted quad, so a partial paste is a no-op and
  // would leave the field valid.
  it("rejects an incomplete fallback router address", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DhcpConfigCard {...cardProps()} />);
    await user.click(screen.getByTestId("dhcp-config-edit"));
    const octets = screen.getByTestId("dhcp-router").querySelectorAll("input");
    await user.click(octets[3]);
    // biome-ignore lint/security/noSecrets: identifier-shaped string, not a credential — the entropy heuristic misfires on long CamelCase names
    await user.keyboard("{Control>}a{/Control}{Backspace}");
    expect(
      await screen.findByText("Enter a complete fallback router address."),
    ).toBeInTheDocument();
  });

  // A public router address would hand every client a gateway off the LAN.
  it("rejects a non-private fallback router address", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DhcpConfigCard {...cardProps()} />);
    await user.click(screen.getByTestId("dhcp-config-edit"));
    const router = screen
      .getByTestId("dhcp-router")
      .querySelectorAll("input")[0];
    await user.click(router);
    await user.paste("8.8.8.8");
    expect(
      await screen.findByText(/Fallback router must be/),
    ).toBeInTheDocument();
  });

  it("edits the lease duration and upstream DNS fields", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DhcpConfigCard {...cardProps()} />);
    await user.click(screen.getByTestId("dhcp-config-edit"));

    const lease = screen.getByLabelText(/Lease duration/i);
    await user.clear(lease);
    await user.type(lease, "3600");
    expect(lease).toHaveValue(3600);

    const dns = screen.getByLabelText(/Upstream DNS/i);
    await user.clear(dns);
    await user.type(dns, "9.9.9.9");
    expect(dns).toHaveValue("9.9.9.9");
  });

  it("renders an API error alert in edit mode when the update failed", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DhcpConfigCard
        {...cardProps({
          update: { isError: true, error: new Error("nope") },
        })}
      />,
    );
    await user.click(screen.getByTestId("dhcp-config-edit"));
    // Edit mode is active and the isError branch renders the alert.
    expect(screen.getByTestId("dhcp-pool-start")).toBeInTheDocument();
    expect(
      screen.getByText(/nope|Failed to update configuration/),
    ).toBeInTheDocument();
  });
});
