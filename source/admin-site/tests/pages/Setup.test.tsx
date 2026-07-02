/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useSetupStatus } = vi.hoisted(() => ({ useSetupStatus: vi.fn() }));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useSetupStatus };
});

// Stub each wizard step + the stepper as tiny markers so we can assert
// exactly which one renders for each `wizard_step`.
vi.mock("@/pages/setup/WizardStepper", () => ({
  WizardStepper: ({ current }: any) => (
    <div data-testid="stepper">stepper:{current}</div>
  ),
}));
vi.mock("@/pages/setup/Step1Admin", () => ({
  default: () => <div>step1-admin</div>,
}));
vi.mock("@/pages/setup/Step2Network", () => ({
  default: () => <div>step2-network</div>,
}));
vi.mock("@/pages/setup/Step3DhcpOnboarding", () => ({
  default: ({ initialMode }: any) => <div>step3-dhcp:{initialMode}</div>,
}));
vi.mock("@/pages/setup/Step4RouterMac", () => ({
  default: () => <div>step4-router-mac</div>,
}));
vi.mock("@/pages/setup/Step5Tunnel", () => ({
  default: () => <div>step5-tunnel</div>,
}));
vi.mock("@/pages/setup/Step6Policy", () => ({
  default: () => <div>step6-policy</div>,
}));
vi.mock("@/pages/setup/Step7RemoteAccess", () => ({
  default: () => <div>step7-remote-access</div>,
}));
vi.mock("@/pages/setup/Step8Confirm", () => ({
  default: () => <div>step8-confirm</div>,
}));

import Setup from "@/pages/Setup";
import { renderWithProviders } from "../test-utils";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("Setup wizard dispatch", () => {
  it("shows Loading while data is not yet available", () => {
    useSetupStatus.mockReturnValue({ data: undefined, isLoading: true });
    renderWithProviders(<Setup />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
    expect(screen.queryByTestId("stepper")).not.toBeInTheDocument();
  });

  it("shows Loading when not loading but data is null", () => {
    useSetupStatus.mockReturnValue({ data: null, isLoading: false });
    renderWithProviders(<Setup />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it.each([
    ["admin", "step1-admin"],
    ["network", "step2-network"],
    ["router_mac", "step4-router-mac"],
    ["tunnel", "step5-tunnel"],
    ["policy", "step6-policy"],
    ["remote_access", "step7-remote-access"],
    ["completed", "step8-confirm"],
  ])("renders the step for wizard_step=%s", (step, marker) => {
    useSetupStatus.mockReturnValue({
      data: { wizard_step: step },
      isLoading: false,
    });
    renderWithProviders(<Setup />);
    expect(screen.getByTestId("stepper")).toHaveTextContent(`stepper:${step}`);
    expect(screen.getByText(marker)).toBeInTheDocument();
  });

  it("passes wizard_mode to Step3DhcpOnboarding for the dhcp step", () => {
    useSetupStatus.mockReturnValue({
      data: { wizard_step: "dhcp", wizard_mode: "managed" },
      isLoading: false,
    });
    renderWithProviders(<Setup />);
    expect(screen.getByText("step3-dhcp:managed")).toBeInTheDocument();
  });
});
