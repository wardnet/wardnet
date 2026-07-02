import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { WizardStep } from "@wardnet/js";

import { WizardStepper } from "@/pages/setup/WizardStepper";
import { renderWithProviders } from "../../test-utils";

describe("WizardStepper", () => {
  it("marks the first step current on admin", () => {
    renderWithProviders(<WizardStepper current="admin" />);
    expect(screen.getByText("Step 1 of 8 — Admin")).toBeInTheDocument();
    const current = screen.getByLabelText("Admin");
    expect(current).toHaveAttribute("aria-current", "step");
  });

  it("renders a middle step with past steps behind it", () => {
    renderWithProviders(<WizardStepper current="dhcp" />);
    expect(screen.getByText("Step 3 of 8 — DHCP")).toBeInTheDocument();
    expect(screen.getByLabelText("DHCP")).toHaveAttribute(
      "aria-current",
      "step",
    );
  });

  it("renders the final completed step", () => {
    renderWithProviders(<WizardStepper current="completed" />);
    expect(screen.getByText("Step 8 of 8 — Done")).toBeInTheDocument();
  });

  it("handles an unknown step gracefully", () => {
    renderWithProviders(
      <WizardStepper current={"nope" as unknown as WizardStep} />,
    );
    // ordinal() returns -1 → index 0, no label.
    expect(screen.getByText(/Step 0 of 8 —/)).toBeInTheDocument();
  });
});
