import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useAdvanceWizard, useDiscoverGatewayMac } = vi.hoisted(() => ({
  useAdvanceWizard: vi.fn(),
  useDiscoverGatewayMac: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useAdvanceWizard, useDiscoverGatewayMac };
});

import Step4RouterMac from "@/pages/setup/Step4RouterMac";
import { renderWithProviders } from "../../test-utils";

const advanceMutate = vi.fn();
const probeMutate = vi.fn();
const probeMutateAsync = vi.fn();

function setProbe(overrides: Record<string, unknown> = {}) {
  useDiscoverGatewayMac.mockReturnValue({
    mutate: probeMutate,
    mutateAsync: probeMutateAsync,
    data: undefined,
    isError: false,
    isPending: false,
    ...overrides,
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  advanceMutate.mockResolvedValue(undefined);
  probeMutateAsync.mockResolvedValue(undefined);
  useAdvanceWizard.mockReturnValue({
    mutateAsync: advanceMutate,
    isPending: false,
  });
  setProbe();
});

describe("Step4RouterMac", () => {
  it("auto-fires the ARP probe on mount", () => {
    renderWithProviders(<Step4RouterMac />);
    expect(probeMutate).toHaveBeenCalledWith({});
  });

  it("shows the probing message while pending with no mac", () => {
    setProbe({ isPending: true });
    renderWithProviders(<Step4RouterMac />);
    expect(
      screen.getByText("Probing the gateway via ARP…"),
    ).toBeInTheDocument();
  });

  it("renders a discovered MAC (ARP) and a Continue button", () => {
    setProbe({ data: { mac: "00:11:22:AA:BB:CC", source: "arp" } });
    renderWithProviders(<Step4RouterMac />);
    expect(screen.getByText("Discovered via ARP")).toBeInTheDocument();
    expect(screen.getByText("00:11:22:AA:BB:CC")).toBeInTheDocument();
    expect(screen.getByTestId("setup-router-mac-continue")).toHaveTextContent(
      "Continue",
    );
  });

  it("labels a non-ARP source as Recorded", () => {
    setProbe({ data: { mac: "00:11:22:AA:BB:CC", source: "manual" } });
    renderWithProviders(<Step4RouterMac />);
    expect(screen.getByText("Recorded")).toBeInTheDocument();
  });

  it("shows the manual-entry form after a failed probe and submits a valid MAC", async () => {
    setProbe({ isError: true });
    const user = userEvent.setup();
    renderWithProviders(<Step4RouterMac />);
    const input = screen.getByPlaceholderText("00:11:22:AA:BB:CC");
    await user.type(input, "00:11:22:AA:BB:CC");
    await user.click(screen.getByRole("button", { name: "Save MAC" }));
    await waitFor(() =>
      expect(probeMutateAsync).toHaveBeenCalledWith({
        mac: "00:11:22:AA:BB:CC",
      }),
    );
    // "Skip" label when no probed mac.
    expect(screen.getByTestId("setup-router-mac-continue")).toHaveTextContent(
      "Skip",
    );
  });

  it("blocks submit of an invalid MAC (mutation not called)", async () => {
    setProbe({ isError: true });
    const user = userEvent.setup();
    renderWithProviders(<Step4RouterMac />);
    await user.type(screen.getByPlaceholderText("00:11:22:AA:BB:CC"), "nope");
    await user.click(screen.getByRole("button", { name: "Save MAC" }));
    await waitFor(() =>
      expect(
        screen.getByText(/Must be a valid MAC address/),
      ).toBeInTheDocument(),
    );
    expect(probeMutateAsync).not.toHaveBeenCalled();
  });

  it("advances to tunnel on continue", async () => {
    setProbe({ data: { mac: "00:11:22:AA:BB:CC", source: "arp" } });
    const user = userEvent.setup();
    renderWithProviders(<Step4RouterMac />);
    await user.click(screen.getByTestId("setup-router-mac-continue"));
    await waitFor(() =>
      expect(advanceMutate).toHaveBeenCalledWith({ to_step: "tunnel" }),
    );
  });

  it("shows 'Saving…' while advance is pending", () => {
    setProbe({ data: { mac: "00:11:22:AA:BB:CC", source: "arp" } });
    useAdvanceWizard.mockReturnValue({
      mutateAsync: advanceMutate,
      isPending: true,
    });
    renderWithProviders(<Step4RouterMac />);
    expect(screen.getByTestId("setup-router-mac-continue")).toHaveTextContent(
      "Saving…",
    );
  });
});
