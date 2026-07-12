import type { ReactNode } from "react";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useAdvanceWizard, useDhcpSelfProbe } = vi.hoisted(() => ({
  useAdvanceWizard: vi.fn(),
  useDhcpSelfProbe: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useAdvanceWizard,
    useDhcpSelfProbe,
    // Replace the radix Select with a native <select> so option changes are
    // driveable in jsdom (exercises the router picker / kb_url branch).
    Select: ({
      value,
      onValueChange,
      children,
    }: {
      value: string;
      onValueChange: (v: string) => void;
      children: ReactNode;
    }) => (
      <select
        data-testid="router-select"
        value={value}
        onChange={(e) => onValueChange(e.target.value)}
      >
        {children}
      </select>
    ),
    SelectItem: ({
      value,
      children,
    }: {
      value: string;
      children: ReactNode;
    }) => <option value={value}>{children}</option>,
    SelectContent: ({ children }: { children: ReactNode }) => <>{children}</>,
    SelectTrigger: () => null,
    SelectValue: () => null,
  };
});

import StepDhcp from "@/pages/setup/StepDhcp";
import { renderWithProviders } from "../../test-utils";

const advanceMutate = vi.fn();
const probeMutate = vi.fn();

function setProbe(overrides: Record<string, unknown> = {}) {
  useDhcpSelfProbe.mockReturnValue({
    mutate: probeMutate,
    data: undefined,
    isError: false,
    isPending: false,
    ...overrides,
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  useAdvanceWizard.mockReturnValue({ mutate: advanceMutate, isPending: false });
  setProbe();
});

describe("StepDhcp", () => {
  it("defaults to primary mode; continue blocked until a clean probe", () => {
    renderWithProviders(<StepDhcp initialMode={null} />);
    expect(screen.getByText("Pick your router")).toBeInTheDocument();
    const cont = screen.getByTestId("setup-dhcp-continue");
    expect(cont).toBeDisabled();
    expect(cont).toHaveTextContent("Run a clean probe to continue");
    // First router (Safaricom) has no kb_url — support link absent.
    expect(screen.queryByText(/support page/)).not.toBeInTheDocument();
  });

  it("shows a support link after switching to a router that has a kb_url", async () => {
    const user = userEvent.setup();
    renderWithProviders(<StepDhcp initialMode="primary" />);
    await user.selectOptions(
      screen.getByTestId("router-select"),
      "vodafone-smart-hub",
    );
    expect(
      screen.getByText(/Vodafone Smart Hub support page/),
    ).toBeInTheDocument();
  });

  it("locked_router mode enables continue and advances with wizard_mode", async () => {
    const user = userEvent.setup();
    renderWithProviders(<StepDhcp initialMode={null} />);
    await user.click(screen.getByTestId("setup-dhcp-mode-locked"));
    expect(
      screen.getByText(/Configure each opted-in device/),
    ).toBeInTheDocument();
    const cont = screen.getByTestId("setup-dhcp-continue");
    expect(cont).not.toBeDisabled();
    await user.click(cont);
    expect(advanceMutate).toHaveBeenCalledWith({
      to_step: "router_mac",
      wizard_mode: "locked_router",
    });
  });

  it("shows a clean-probe success and advances in primary mode", async () => {
    setProbe({ data: { wardnet_responded: true, foreign_responded: false } });
    const user = userEvent.setup();
    renderWithProviders(<StepDhcp initialMode="primary" />);
    expect(
      screen.getByText("Only Wardnet responded - good."),
    ).toBeInTheDocument();
    const cont = screen.getByTestId("setup-dhcp-continue");
    expect(cont).not.toBeDisabled();
    await user.click(cont);
    expect(advanceMutate).toHaveBeenCalledWith({
      to_step: "router_mac",
      wizard_mode: "primary",
    });
  });

  it("shows a foreign-server warning when the probe is blocked", () => {
    setProbe({
      data: {
        wardnet_responded: true,
        foreign_responded: true,
        foreign_server_ip: "192.168.1.1",
      },
    });
    renderWithProviders(<StepDhcp initialMode="primary" />);
    expect(
      screen.getByText(/A foreign DHCP server responded \(192\.168\.1\.1\)/),
    ).toBeInTheDocument();
  });

  it("shows a probe-failed message on error", () => {
    setProbe({ isError: true });
    renderWithProviders(<StepDhcp initialMode="primary" />);
    expect(screen.getByText("Probe failed - try again.")).toBeInTheDocument();
  });

  it("shows 'Probing…' and disables the probe button while pending", () => {
    setProbe({ isPending: true });
    renderWithProviders(<StepDhcp initialMode="primary" />);
    expect(screen.getByRole("button", { name: "Probing…" })).toBeDisabled();
  });

  it("clicking Probe LAN calls the probe mutation; label becomes 'Probe again'", async () => {
    const user = userEvent.setup();
    const { rerender } = renderWithProviders(
      <StepDhcp initialMode="primary" />,
    );
    await user.click(screen.getByRole("button", { name: "Probe LAN" }));
    expect(probeMutate).toHaveBeenCalled();

    setProbe({ data: { wardnet_responded: false, foreign_responded: false } });
    rerender(<StepDhcp initialMode="primary" />);
    expect(
      screen.getByRole("button", { name: "Probe again" }),
    ).toBeInTheDocument();
  });

  it("shows 'Saving…' while advance is pending", () => {
    setProbe({ data: { wardnet_responded: true, foreign_responded: false } });
    useAdvanceWizard.mockReturnValue({
      mutate: advanceMutate,
      isPending: true,
    });
    renderWithProviders(<StepDhcp initialMode="primary" />);
    expect(screen.getByTestId("setup-dhcp-continue")).toHaveTextContent(
      "Saving…",
    );
  });
});
