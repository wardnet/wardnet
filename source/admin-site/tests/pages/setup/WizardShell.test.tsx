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

import { WizardShell } from "@/pages/setup/WizardShell";
import { WizardFooter } from "@/pages/setup/WizardFooter";
import { renderWithProviders } from "../../test-utils";

const advanceMutate = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  useAdvanceWizard.mockReturnValue({ mutate: advanceMutate, isPending: false });
});

describe("WizardShell", () => {
  it("portals WizardFooter content into the docked footer region", () => {
    renderWithProviders(
      <WizardShell current="network">
        <div>body content</div>
        <WizardFooter>
          <button type="button">CTA</button>
        </WizardFooter>
      </WizardShell>,
    );
    const footer = screen.getByTestId("setup-wizard-footer");
    const cta = screen.getByRole("button", { name: "CTA" });
    expect(footer.contains(cta)).toBe(true);
  });

  it("renders WizardFooter inline when no shell is mounted", () => {
    renderWithProviders(
      <WizardFooter>
        <button type="button">standalone CTA</button>
      </WizardFooter>,
    );
    expect(
      screen.getByRole("button", { name: "standalone CTA" }),
    ).toBeInTheDocument();
  });

  it("shows the back link on rewindable steps and rewinds on click", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <WizardShell current="dhcp">
        <div>body</div>
      </WizardShell>,
    );
    await user.click(screen.getByTestId("setup-back"));
    expect(advanceMutate).toHaveBeenCalledWith({ to_step: "network" });
  });

  it("hides the back link at the rewind floor and when completed", () => {
    const { unmount } = renderWithProviders(
      <WizardShell current="network">
        <div>body</div>
      </WizardShell>,
    );
    expect(screen.queryByTestId("setup-back")).not.toBeInTheDocument();
    unmount();

    renderWithProviders(
      <WizardShell current="completed">
        <div>body</div>
      </WizardShell>,
    );
    expect(screen.queryByTestId("setup-back")).not.toBeInTheDocument();
  });

  it("hides the back link on the admin step", () => {
    renderWithProviders(
      <WizardShell current="admin">
        <div>body</div>
      </WizardShell>,
    );
    expect(screen.queryByTestId("setup-back")).not.toBeInTheDocument();
  });
});
