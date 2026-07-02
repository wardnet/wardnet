import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useAdvanceWizard, useNetworkStatus } = vi.hoisted(() => ({
  useAdvanceWizard: vi.fn(),
  useNetworkStatus: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useAdvanceWizard, useNetworkStatus };
});

import Step2Network from "@/pages/setup/Step2Network";
import { renderWithProviders } from "../../test-utils";

const advanceMutate = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  useAdvanceWizard.mockReturnValue({ mutate: advanceMutate, isPending: false });
  useNetworkStatus.mockReturnValue({
    data: undefined,
    isLoading: false,
    isError: false,
  });
});

describe("Step2Network", () => {
  it("shows placeholders while loading", () => {
    useNetworkStatus.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    });
    renderWithProviders(<Step2Network />);
    expect(screen.getAllByText("…").length).toBeGreaterThan(0);
  });

  it("shows dashes on error", () => {
    useNetworkStatus.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
    });
    renderWithProviders(<Step2Network />);
    expect(screen.getAllByText("—").length).toBeGreaterThan(0);
  });

  it("renders static source with no warning panel", () => {
    useNetworkStatus.mockReturnValue({
      data: {
        interface: "eth0",
        ip: "10.0.0.2",
        gateway: "10.0.0.1",
        dhcp_source: "static",
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<Step2Network />);
    expect(screen.getByText("Static (install.sh)")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("renders dhcp source with the not-pinned warning panel", () => {
    useNetworkStatus.mockReturnValue({
      data: {
        interface: "eth0",
        ip: "10.0.0.2",
        gateway: "10.0.0.1",
        dhcp_source: "dhcp",
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<Step2Network />);
    expect(screen.getByText("DHCP (router-assigned)")).toBeInTheDocument();
    expect(screen.getByRole("status")).toBeInTheDocument();
    expect(screen.getByText("Your IP isn't pinned")).toBeInTheDocument();
  });

  it("renders unknown source and warning when dhcp_source is absent", () => {
    useNetworkStatus.mockReturnValue({
      data: { interface: "eth0", ip: null, gateway: null },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<Step2Network />);
    expect(screen.getByText("Unknown")).toBeInTheDocument();
    expect(screen.getByText("not detected")).toBeInTheDocument();
    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  it("advances to dhcp on continue", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Step2Network />);
    await user.click(screen.getByTestId("setup-network-continue"));
    expect(advanceMutate).toHaveBeenCalledWith({ to_step: "dhcp" });
  });

  it("shows saving label and disables continue while pending", () => {
    useAdvanceWizard.mockReturnValue({
      mutate: advanceMutate,
      isPending: true,
    });
    renderWithProviders(<Step2Network />);
    const btn = screen.getByTestId("setup-network-continue");
    expect(btn).toBeDisabled();
    expect(btn).toHaveTextContent("Saving…");
  });
});
