import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useAdvanceWizard } = vi.hoisted(() => ({
  useAdvanceWizard: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useAdvanceWizard };
});

import { WizardRail } from "@/pages/setup/WizardRail";
import { renderWithProviders } from "../../test-utils";

const advanceMutate = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  useAdvanceWizard.mockReturnValue({ mutate: advanceMutate, isPending: false });
});

describe("WizardRail", () => {
  it("renders all ten steps with the current one marked", () => {
    renderWithProviders(<WizardRail current="dns" />);
    const current = screen.getByTestId("setup-rail-dns");
    expect(current).toHaveAttribute("aria-current", "step");
    expect(screen.getByTestId("setup-rail-admin")).toBeInTheDocument();
    expect(screen.getByTestId("setup-rail-completed")).toBeInTheDocument();
    expect(screen.getByText("Review")).toBeInTheDocument();
  });

  it("rewinds when a done step row is clicked", async () => {
    const user = userEvent.setup();
    renderWithProviders(<WizardRail current="policy" />);
    await user.click(screen.getByTestId("setup-rail-network"));
    expect(advanceMutate).toHaveBeenCalledWith({ to_step: "network" });
  });

  it("never rewinds to admin (rewind floor)", () => {
    renderWithProviders(<WizardRail current="policy" />);
    expect(screen.getByTestId("setup-rail-admin")).toBeDisabled();
  });

  it("keeps upcoming steps inert (no forward jumps)", () => {
    renderWithProviders(<WizardRail current="dhcp" />);
    expect(screen.getByTestId("setup-rail-tunnel")).toBeDisabled();
    expect(screen.getByTestId("setup-rail-dhcp")).toBeDisabled();
  });

  it("disables every row once completed (terminal)", () => {
    renderWithProviders(<WizardRail current="completed" />);
    for (const id of ["network", "dhcp", "review"]) {
      expect(screen.getByTestId(`setup-rail-${id}`)).toBeDisabled();
    }
  });

  it("shows the mobile progress caption for the current step", () => {
    renderWithProviders(<WizardRail current="dns" />);
    expect(screen.getByText("Step 5 of 10 · DNS")).toBeInTheDocument();
    expect(screen.getByText("50%")).toBeInTheDocument();
  });
});
