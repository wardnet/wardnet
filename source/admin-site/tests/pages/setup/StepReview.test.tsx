import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useAdvanceWizard,
  useMe,
  useNetworkStatus,
  useSetupStatus,
  useDnsFilterConfig,
  useDnsFilterProfiles,
  useTunnels,
  useDefaultPolicy,
  useDdnsStatus,
} = vi.hoisted(() => ({
  useAdvanceWizard: vi.fn(),
  useMe: vi.fn(),
  useNetworkStatus: vi.fn(),
  useSetupStatus: vi.fn(),
  useDnsFilterConfig: vi.fn(),
  useDnsFilterProfiles: vi.fn(),
  useTunnels: vi.fn(),
  useDefaultPolicy: vi.fn(),
  useDdnsStatus: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useAdvanceWizard,
    useMe,
    useNetworkStatus,
    useSetupStatus,
    useDnsFilterConfig,
    useDnsFilterProfiles,
    useTunnels,
    useDefaultPolicy,
    useDdnsStatus,
  };
});

import StepReview from "@/pages/setup/StepReview";
import { renderWithProviders } from "../../test-utils";

const advanceMutate = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  useAdvanceWizard.mockReturnValue({ mutate: advanceMutate, isPending: false });
  useMe.mockReturnValue({ data: { username: "operator" } });
  useNetworkStatus.mockReturnValue({
    data: {
      interface: "eth0",
      ip: "192.168.1.2",
      gateway: "192.168.1.1",
      dhcp_source: "static",
      router_mac: "00:11:22:AA:BB:CC",
    },
  });
  useSetupStatus.mockReturnValue({
    data: { wizard_step: "review", wizard_mode: "primary" },
  });
  useDnsFilterConfig.mockReturnValue({
    data: { config: { enabled: true, default_profile_ids: ["p-1"] } },
  });
  useDnsFilterProfiles.mockReturnValue({
    data: { profiles: [{ id: "p-1", name: "Ad Blocking", builtin: true }] },
  });
  useTunnels.mockReturnValue({
    data: { tunnels: [{ id: "t-1", label: "Proton NL" }] },
  });
  useDefaultPolicy.mockReturnValue({ data: { policy: "t-1" } });
  useDdnsStatus.mockReturnValue({
    data: { provider: "wardnet", fqdn: "home.my.wardnet.services" },
  });
});

describe("StepReview", () => {
  it("summarises every configured value", () => {
    renderWithProviders(<StepReview />);
    expect(screen.getByText("operator")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.2")).toBeInTheDocument();
    expect(screen.getByText("Primary")).toBeInTheDocument();
    expect(screen.getByText("00:11:22:AA:BB:CC")).toBeInTheDocument();
    expect(screen.getByText("Ad Blocking")).toBeInTheDocument();
    expect(screen.getAllByText("Proton NL").length).toBeGreaterThan(0);
    expect(screen.getByText("home.my.wardnet.services")).toBeInTheDocument();
  });

  it("shows Skipped for absent router MAC and remote access", () => {
    useNetworkStatus.mockReturnValue({
      data: {
        interface: "eth0",
        ip: "192.168.1.2",
        gateway: null,
        dhcp_source: "static",
        router_mac: null,
      },
    });
    useDdnsStatus.mockReturnValue({ data: { provider: null, fqdn: null } });
    renderWithProviders(<StepReview />);
    expect(screen.getAllByText("Skipped")).toHaveLength(2);
  });

  it("summarises direct routing and no tunnels", () => {
    useTunnels.mockReturnValue({ data: { tunnels: [] } });
    useDefaultPolicy.mockReturnValue({ data: { policy: "direct" } });
    renderWithProviders(<StepReview />);
    expect(screen.getByText("None")).toBeInTheDocument();
    expect(screen.getByText("Direct (no VPN)")).toBeInTheDocument();
  });

  it("shows Disabled when DNS filtering is off", () => {
    useDnsFilterConfig.mockReturnValue({
      data: { config: { enabled: false, default_profile_ids: [] } },
    });
    renderWithProviders(<StepReview />);
    expect(screen.getByText("Disabled")).toBeInTheDocument();
  });

  it("offers no Edit link on the Admin row", () => {
    renderWithProviders(<StepReview />);
    expect(
      screen.queryByTestId("setup-review-edit-admin"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("setup-review-edit-dns")).toBeInTheDocument();
  });

  it("rewinds to the row's step when Edit is clicked", async () => {
    const user = userEvent.setup();
    renderWithProviders(<StepReview />);
    await user.click(screen.getByTestId("setup-review-edit-dhcp"));
    expect(advanceMutate).toHaveBeenCalledWith({ to_step: "dhcp" });
  });

  it("finishes setup by advancing to completed", async () => {
    const user = userEvent.setup();
    renderWithProviders(<StepReview />);
    await user.click(screen.getByTestId("setup-review-finish"));
    expect(advanceMutate).toHaveBeenCalledWith({ to_step: "completed" });
  });
});
