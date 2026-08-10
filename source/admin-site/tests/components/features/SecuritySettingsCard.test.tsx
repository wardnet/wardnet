import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SecuritySettingsCard } from "@/components/features/SecuritySettingsCard";
import { renderWithProviders } from "../../test-utils";
import type { DnsConfig } from "@wardnet/js";

const onUpdate = vi.fn();

function renderCard(
  config: Record<string, unknown> | undefined,
  isLoading = false,
) {
  return renderWithProviders(
    <SecuritySettingsCard
      config={config as DnsConfig | undefined}
      isLoading={isLoading}
      onUpdate={onUpdate}
      updatePending={false}
    />,
  );
}

describe("SecuritySettingsCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("reflects the loaded config in the toggles", () => {
    renderCard({
      dnssec_enabled: true,
      rebinding_protection: false,
      rate_limit_per_second: 50,
    });

    expect(screen.getByLabelText("Enable DNSSEC validation")).toBeChecked();
    expect(
      screen.getByLabelText("Enable DNS rebinding protection"),
    ).not.toBeChecked();
    expect(screen.getByRole("spinbutton")).toHaveValue(50);
  });

  it("calls the update callback when DNSSEC and rebinding toggles change", async () => {
    renderCard({ dnssec_enabled: false, rebinding_protection: true });

    await userEvent.click(screen.getByLabelText("Enable DNSSEC validation"));
    expect(onUpdate).toHaveBeenCalledWith({ dnssec_enabled: true });

    await userEvent.click(
      screen.getByLabelText("Enable DNS rebinding protection"),
    );
    expect(onUpdate).toHaveBeenCalledWith({ rebinding_protection: false });
  });

  it("shows Save when the rate limit is dirty and saves the parsed value", async () => {
    renderCard({ rate_limit_per_second: 0 });

    // No Save button until the rate is edited to a new value.
    expect(
      screen.queryByRole("button", { name: "Save" }),
    ).not.toBeInTheDocument();

    const input = screen.getByRole("spinbutton");
    await userEvent.clear(input);
    await userEvent.type(input, "25");

    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(onUpdate).toHaveBeenCalledWith(
      { rate_limit_per_second: 25 },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("shows a validation error and disables Save for a bad value", async () => {
    renderCard({ rate_limit_per_second: 0 });

    const input = screen.getByRole("spinbutton");
    await userEvent.clear(input);
    await userEvent.type(input, "-5");

    expect(screen.getByText("Enter a whole number ≥ 0.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("cancel resets the rate edit buffer", async () => {
    renderCard({ rate_limit_per_second: 10 });

    const input = screen.getByRole("spinbutton");
    await userEvent.clear(input);
    await userEvent.type(input, "99");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    // Reverts to the config value and the actions row disappears.
    expect(screen.getByRole("spinbutton")).toHaveValue(10);
    expect(
      screen.queryByRole("button", { name: "Save" }),
    ).not.toBeInTheDocument();
  });

  it("disables the controls while the config loads or an update is pending", () => {
    renderCard({ rate_limit_per_second: 0 }, true);
    expect(screen.getByLabelText("Enable DNSSEC validation")).toBeDisabled();
    expect(screen.getByRole("spinbutton")).toBeDisabled();
  });
});
