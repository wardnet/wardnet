/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useSetupStatus } = vi.hoisted(() => ({ useSetupStatus: vi.fn() }));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useSetupStatus };
});

// Stub each wizard step + the shell as tiny markers so we can assert
// exactly which one renders for each `wizard_step`.
vi.mock("@/pages/setup/WizardShell", () => ({
  WizardShell: ({ current, children }: any) => (
    <div data-testid="shell">
      <div data-testid="shell-current">shell:{current}</div>
      {children}
    </div>
  ),
}));
vi.mock("@/pages/setup/StepAdmin", () => ({
  default: () => <div>step1-admin</div>,
}));
vi.mock("@/pages/setup/StepNetwork", () => ({
  default: () => <div>step2-network</div>,
}));
vi.mock("@/pages/setup/StepDhcp", () => ({
  default: ({ initialMode }: any) => <div>step3-dhcp:{initialMode}</div>,
}));
vi.mock("@/pages/setup/StepRouterMac", () => ({
  default: () => <div>step4-router-mac</div>,
}));
vi.mock("@/pages/setup/StepDns", () => ({
  default: () => <div>step5-dns</div>,
}));
vi.mock("@/pages/setup/StepTunnel", () => ({
  default: () => <div>step5-tunnel</div>,
}));
vi.mock("@/pages/setup/StepPolicy", () => ({
  default: () => <div>step6-policy</div>,
}));
vi.mock("@/pages/setup/StepRemoteAccess", () => ({
  default: () => <div>step7-remote-access</div>,
}));
vi.mock("@/pages/setup/StepReview", () => ({
  default: () => <div>step9-review</div>,
}));
vi.mock("@/pages/setup/StepDone", () => ({
  default: () => <div>step10-done</div>,
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
    // The shell renders from first paint so the card shape is stable.
    expect(screen.getByTestId("shell")).toBeInTheDocument();
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
    ["dns", "step5-dns"],
    ["tunnel", "step5-tunnel"],
    ["policy", "step6-policy"],
    ["remote_access", "step7-remote-access"],
    ["review", "step9-review"],
    ["completed", "step10-done"],
  ])("renders the step for wizard_step=%s", (step, marker) => {
    useSetupStatus.mockReturnValue({
      data: { wizard_step: step },
      isLoading: false,
    });
    renderWithProviders(<Setup />);
    expect(screen.getByTestId("shell-current")).toHaveTextContent(
      `shell:${step}`,
    );
    expect(screen.getByText(marker)).toBeInTheDocument();
  });

  it("passes wizard_mode to StepDhcp for the dhcp step", () => {
    useSetupStatus.mockReturnValue({
      data: { wizard_step: "dhcp", wizard_mode: "managed" },
      isLoading: false,
    });
    renderWithProviders(<Setup />);
    expect(screen.getByText("step3-dhcp:managed")).toBeInTheDocument();
  });
});
