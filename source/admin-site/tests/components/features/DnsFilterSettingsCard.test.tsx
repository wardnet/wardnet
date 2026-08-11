import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { DnsFilterConfig } from "@wardnet/js";
import { DnsFilterSettingsCard } from "@/components/features/DnsFilterSettingsCard";
import { renderWithProviders } from "../../test-utils";

function makeConfig(over: Partial<DnsFilterConfig> = {}): DnsFilterConfig {
  return {
    enabled: true,
    default_profile_ids: [],
    blocklist_failure_alert_threshold: 5,
    ...over,
  };
}

describe("DnsFilterSettingsCard", () => {
  it("shows Enabled and a checked toggle when filtering is on", () => {
    renderWithProviders(
      <DnsFilterSettingsCard
        config={makeConfig({ enabled: true })}
        isLoading={false}
        onToggle={vi.fn()}
        onThresholdSave={vi.fn()}
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
        onThresholdSave={vi.fn()}
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
        onThresholdSave={vi.fn()}
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
        onThresholdSave={vi.fn()}
      />,
    );
    expect(screen.getByLabelText("Enable DNS filtering")).toBeDisabled();
  });

  it("saves an edited failure-alert threshold", async () => {
    const onThresholdSave = vi.fn();
    renderWithProviders(
      <DnsFilterSettingsCard
        config={makeConfig()}
        isLoading={false}
        onToggle={vi.fn()}
        onThresholdSave={onThresholdSave}
      />,
    );

    // Save only appears once the value actually differs from the server's.
    expect(
      screen.queryByTestId("save-blocklist-failure-alert-threshold"),
    ).not.toBeInTheDocument();

    const input = screen.getByTestId("blocklist-failure-alert-threshold");
    await userEvent.clear(input);
    await userEvent.type(input, "10");
    await userEvent.click(
      screen.getByTestId("save-blocklist-failure-alert-threshold"),
    );

    expect(onThresholdSave).toHaveBeenCalledWith(10);
  });

  // Cancel exists so an admin can back out of an edit without having to
  // remember what the server value was.
  it("restores the server value when an edit is cancelled", async () => {
    renderWithProviders(
      <DnsFilterSettingsCard
        config={makeConfig({ blocklist_failure_alert_threshold: 5 })}
        isLoading={false}
        onToggle={vi.fn()}
        onThresholdSave={vi.fn()}
      />,
    );

    const input = screen.getByTestId("blocklist-failure-alert-threshold");
    await userEvent.clear(input);
    await userEvent.type(input, "10");
    expect(input).toHaveValue(10);

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(input).toHaveValue(5);
    expect(
      screen.queryByTestId("save-blocklist-failure-alert-threshold"),
    ).not.toBeInTheDocument();
  });

  // A completed save — or another admin changing the setting — must not be
  // masked by a stale buffer still showing the old edit.
  it("drops a stale edit when the server value changes underneath it", async () => {
    const { rerender } = renderWithProviders(
      <DnsFilterSettingsCard
        config={makeConfig({ blocklist_failure_alert_threshold: 5 })}
        isLoading={false}
        onToggle={vi.fn()}
        onThresholdSave={vi.fn()}
      />,
    );

    const input = screen.getByTestId("blocklist-failure-alert-threshold");
    await userEvent.clear(input);
    await userEvent.type(input, "10");
    expect(input).toHaveValue(10);

    rerender(
      <DnsFilterSettingsCard
        config={makeConfig({ blocklist_failure_alert_threshold: 7 })}
        isLoading={false}
        onToggle={vi.fn()}
        onThresholdSave={vi.fn()}
      />,
    );

    expect(input).toHaveValue(7);
  });

  it("does not offer to save an empty or negative threshold", async () => {
    renderWithProviders(
      <DnsFilterSettingsCard
        config={makeConfig()}
        isLoading={false}
        onToggle={vi.fn()}
        onThresholdSave={vi.fn()}
      />,
    );

    const input = screen.getByTestId("blocklist-failure-alert-threshold");
    await userEvent.clear(input);
    expect(
      screen.getByTestId("save-blocklist-failure-alert-threshold"),
    ).toBeDisabled();
  });
});
