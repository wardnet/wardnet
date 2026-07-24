import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { DnsFilterConfig } from "@wardnet/js";
import { DnsFilterSettingsCard } from "@/components/features/DnsFilterSettingsCard";
import { renderWithProviders } from "../../test-utils";

function makeConfig(over: Partial<DnsFilterConfig> = {}): DnsFilterConfig {
  return { enabled: true, default_profile_ids: [], ...over };
}

describe("DnsFilterSettingsCard", () => {
  it("shows Enabled and a checked toggle when filtering is on", () => {
    renderWithProviders(
      <DnsFilterSettingsCard
        config={makeConfig({ enabled: true })}
        isLoading={false}
        onToggle={vi.fn()}
      />,
    );

    expect(screen.getByText("Enabled")).toBeInTheDocument();
    expect(screen.getByLabelText("Enable DNS filtering")).toBeChecked();
  });

  it("shows Disabled when filtering is off and toggles it on", async () => {
    const onToggle = vi.fn();
    renderWithProviders(
      <DnsFilterSettingsCard
        config={makeConfig({ enabled: false })}
        isLoading={false}
        onToggle={onToggle}
      />,
    );

    expect(screen.getByText("Disabled")).toBeInTheDocument();
    const toggle = screen.getByLabelText("Enable DNS filtering");
    expect(toggle).not.toBeChecked();

    await userEvent.click(toggle);
    expect(onToggle).toHaveBeenCalledWith(true);
  });

  it("treats a missing config as disabled", () => {
    renderWithProviders(
      <DnsFilterSettingsCard
        config={undefined}
        isLoading={false}
        onToggle={vi.fn()}
      />,
    );
    expect(screen.getByText("Disabled")).toBeInTheDocument();
  });

  it("disables the toggle while loading or updating", () => {
    renderWithProviders(
      <DnsFilterSettingsCard
        config={makeConfig()}
        isLoading
        onToggle={vi.fn()}
      />,
    );
    expect(screen.getByLabelText("Enable DNS filtering")).toBeDisabled();
  });
});
