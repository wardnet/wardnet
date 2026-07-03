import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DhcpConfig } from "@wardnet/js";
import { DhcpConfigCard } from "@/components/compound/DhcpConfigCard";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useUpdateDhcpConfig: vi.fn(),
    usePreviewDhcpConfig: vi.fn(),
    useDnsConfig: vi.fn(),
  };
});

import {
  useUpdateDhcpConfig,
  usePreviewDhcpConfig,
  useDnsConfig,
} from "@wardnet/web";

const mockUpdate = vi.mocked(useUpdateDhcpConfig);
const mockPreview = vi.mocked(usePreviewDhcpConfig);
const mockDns = vi.mocked(useDnsConfig);

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

beforeEach(() => {
  updateMutateAsync.mockReset().mockResolvedValue(undefined);
  previewMutateAsync.mockReset().mockResolvedValue({ affected: [] });
  updateReset.mockReset();
  mockUpdate.mockReturnValue({
    mutateAsync: updateMutateAsync,
    reset: updateReset,
    isPending: false,
    isError: false,
    error: null,
  } as never);
  mockPreview.mockReturnValue({
    mutateAsync: previewMutateAsync,
    isPending: false,
  } as never);
  mockDns.mockReturnValue({ data: { config: { enabled: false } } } as never);
});

describe("DhcpConfigCard", () => {
  it("renders the read view with pool range and upstream DNS", () => {
    renderWithProviders(<DhcpConfigCard config={makeConfig()} />);
    expect(screen.getByTestId("dhcp-config-pool-range")).toHaveTextContent(
      "10.232.1.100",
    );
    expect(screen.getByText("1d")).toBeInTheDocument();
    expect(screen.getByText("1.1.1.1, 8.8.8.8")).toBeInTheDocument();
  });

  it("formats sub-hour and sub-day lease durations", () => {
    const { rerender } = renderWithProviders(
      <DhcpConfigCard config={makeConfig({ lease_duration_secs: 1800 })} />,
    );
    expect(screen.getByText("30m")).toBeInTheDocument();
    rerender(
      <DhcpConfigCard config={makeConfig({ lease_duration_secs: 7200 })} />,
    );
    expect(screen.getByText("2h")).toBeInTheDocument();
  });

  it("shows 'Wardnet DNS' in the read view when the DNS server is enabled", () => {
    mockDns.mockReturnValue({ data: { config: { enabled: true } } } as never);
    renderWithProviders(<DhcpConfigCard config={makeConfig()} />);
    expect(screen.getByText("Wardnet DNS")).toBeInTheDocument();
  });

  it("enters edit mode and hides the upstream DNS field when DNS is enabled", async () => {
    const user = userEvent.setup();
    mockDns.mockReturnValue({ data: { config: { enabled: true } } } as never);
    renderWithProviders(<DhcpConfigCard config={makeConfig()} />);
    await user.click(screen.getByTestId("dhcp-config-edit"));
    expect(screen.getByTestId("dhcp-pool-start")).toBeInTheDocument();
    expect(screen.queryByLabelText(/Upstream DNS/)).not.toBeInTheDocument();
  });

  it("surfaces a validation error for an out-of-order pool range", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DhcpConfigCard
        config={makeConfig({
          pool_start: "10.232.1.200",
          pool_end: "10.232.1.100",
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
        config={makeConfig({
          pool_start: "8.8.8.100",
          pool_end: "8.8.8.200",
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
    renderWithProviders(<DhcpConfigCard config={makeConfig()} />);
    await user.click(screen.getByTestId("dhcp-config-edit"));
    await user.click(screen.getByTestId("dhcp-config-cancel"));
    expect(screen.queryByTestId("dhcp-pool-start")).not.toBeInTheDocument();
    expect(updateReset).toHaveBeenCalled();
  });

  it("saves directly when the pool range is unchanged", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DhcpConfigCard config={makeConfig()} />);
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
    renderWithProviders(<DhcpConfigCard config={makeConfig()} />);
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

  it("renders an API error alert in edit mode when the update failed", async () => {
    const user = userEvent.setup();
    mockUpdate.mockReturnValue({
      mutateAsync: updateMutateAsync,
      reset: updateReset,
      isPending: false,
      isError: true,
      error: new Error("nope"),
    } as never);
    renderWithProviders(<DhcpConfigCard config={makeConfig()} />);
    await user.click(screen.getByTestId("dhcp-config-edit"));
    // Edit mode is active and the isError branch renders the alert.
    expect(screen.getByTestId("dhcp-pool-start")).toBeInTheDocument();
    expect(
      screen.getByText(/nope|Failed to update configuration/),
    ).toBeInTheDocument();
  });
});
