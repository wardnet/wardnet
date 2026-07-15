import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { UpdateStatus } from "@wardnet/js";
import { UpdateCard } from "@/components/features/UpdateCard";
import { renderWithProviders } from "../../test-utils";

Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.setPointerCapture ??= () => {};
Element.prototype.releasePointerCapture ??= () => {};
Element.prototype.scrollIntoView ??= () => {};

function makeStatus(overrides: Partial<UpdateStatus> = {}): UpdateStatus {
  return {
    current_version: "1.2.3",
    latest_version: "1.3.0",
    channel: "stable",
    auto_update_enabled: false,
    update_available: false,
    rollback_available: false,
    edge_available: false,
    pending_version: null,
    last_check_at: "2026-01-01T00:00:00Z",
    install_phase: { phase: "idle" },
    ...overrides,
  } as unknown as UpdateStatus;
}

const noop = {
  onCheck: vi.fn(),
  onInstall: vi.fn(),
  onRollback: vi.fn(),
  onToggleAutoUpdate: vi.fn(),
  onChangeChannel: vi.fn(),
};

function baseProps() {
  return {
    status: makeStatus(),
    isLoading: false,
    isChecking: false,
    isInstalling: false,
    isRollingBack: false,
    ...noop,
  };
}

describe("UpdateCard", () => {
  it("shows loading text before a status arrives", () => {
    renderWithProviders(
      <UpdateCard {...baseProps()} status={null} isLoading />,
    );
    expect(screen.getByText("Loading…")).toBeInTheDocument();
    expect(screen.getByText("Disabled")).toBeInTheDocument();
  });

  it("renders version / latest / last-checked details", () => {
    renderWithProviders(<UpdateCard {...baseProps()} />);
    expect(screen.getByText("1.2.3")).toBeInTheDocument();
    expect(screen.getByText("1.3.0")).toBeInTheDocument();
    expect(screen.getByText("Idle")).toBeInTheDocument();
  });

  it("falls back for unknown latest and never-checked", () => {
    renderWithProviders(
      <UpdateCard
        {...baseProps()}
        status={makeStatus({ latest_version: null, last_check_at: null })}
      />,
    );
    expect(screen.getByText("unknown")).toBeInTheDocument();
    expect(screen.getByText("never")).toBeInTheDocument();
  });

  it("shows enabled pill and toggles auto-update", async () => {
    const user = userEvent.setup();
    const onToggleAutoUpdate = vi.fn();
    renderWithProviders(
      <UpdateCard
        {...baseProps()}
        status={makeStatus({ auto_update_enabled: true })}
        onToggleAutoUpdate={onToggleAutoUpdate}
      />,
    );
    expect(screen.getByText("Enabled")).toBeInTheDocument();
    await user.click(screen.getByTestId("update-auto-toggle"));
    expect(onToggleAutoUpdate).toHaveBeenCalledWith(false);
  });

  it("fires onCheck when checking for updates", async () => {
    const user = userEvent.setup();
    const onCheck = vi.fn();
    renderWithProviders(<UpdateCard {...baseProps()} onCheck={onCheck} />);
    await user.click(screen.getByTestId("update-check"));
    expect(onCheck).toHaveBeenCalled();
  });

  it("shows checking state", () => {
    renderWithProviders(<UpdateCard {...baseProps()} isChecking />);
    expect(screen.getByText("Checking…")).toBeInTheDocument();
  });

  it("offers install when an update is available", async () => {
    const user = userEvent.setup();
    const onInstall = vi.fn();
    renderWithProviders(
      <UpdateCard
        {...baseProps()}
        status={makeStatus({ update_available: true })}
        onInstall={onInstall}
      />,
    );
    await user.click(screen.getByRole("button", { name: /Install update/ }));
    expect(onInstall).toHaveBeenCalled();
  });

  it("offers rollback and reports the pending version", async () => {
    const user = userEvent.setup();
    const onRollback = vi.fn();
    renderWithProviders(
      <UpdateCard
        {...baseProps()}
        status={makeStatus({
          rollback_available: true,
          pending_version: "1.3.0",
        })}
        onRollback={onRollback}
      />,
    );
    expect(screen.getByText("Pending")).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Rollback to previous" }),
    );
    expect(onRollback).toHaveBeenCalled();
  });

  it("shows an in-progress button while a downloading phase is active", () => {
    renderWithProviders(
      <UpdateCard
        {...baseProps()}
        status={makeStatus({
          install_phase: { phase: "downloading", bytes: 1024, total: 4096 },
        })}
      />,
    );
    // Appears in the phase cell and in the disabled in-progress button.
    expect(
      screen.getAllByText(/Downloading \(1,024 \/ 4,096 bytes\)/).length,
    ).toBeGreaterThanOrEqual(1);
  });

  it("renders a failure alert with the reason", () => {
    renderWithProviders(
      <UpdateCard
        {...baseProps()}
        status={makeStatus({
          install_phase: { phase: "failed", reason: "checksum mismatch" },
        })}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("checksum mismatch");
    expect(screen.getByText("Failed")).toBeInTheDocument();
  });

  it.each([
    ["checking", "Checking"],
    ["verifying", "Verifying"],
    ["staging", "Staging"],
    ["swapping", "Swapping"],
    ["restart_pending", "Restart pending"],
    ["applied", "Applied"],
  ])("describes the %s phase", (phase, label) => {
    renderWithProviders(
      <UpdateCard
        {...baseProps()}
        status={makeStatus({ install_phase: { phase } as never })}
      />,
    );
    expect(
      // eslint-disable-next-line security/detect-non-literal-regexp -- pattern is a phase label literal from the local it.each table
      screen.getAllByText(new RegExp(label)).length,
    ).toBeGreaterThanOrEqual(1);
  });

  it("describes a downloading phase without a known total", () => {
    renderWithProviders(
      <UpdateCard
        {...baseProps()}
        status={makeStatus({
          install_phase: { phase: "downloading", bytes: 1024, total: null },
        })}
      />,
    );
    expect(screen.getAllByText("Downloading…").length).toBeGreaterThanOrEqual(
      1,
    );
  });

  it("changes the release channel", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const onChangeChannel = vi.fn();
    renderWithProviders(
      <UpdateCard {...baseProps()} onChangeChannel={onChangeChannel} />,
    );
    await user.click(screen.getByTestId("update-channel-trigger"));
    await user.click(
      await screen.findByRole("option", { name: "Beta channel" }),
    );
    expect(onChangeChannel).toHaveBeenCalledWith("beta");
  });

  it("hides the edge channel unless the daemon says the box allows it", async () => {
    // Edge needs `[update] allow_edge_channel` in the box's TOML, which the UI
    // cannot set. Offering the option anyway would just be a button that 403s.
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(
      <UpdateCard
        {...baseProps()}
        status={makeStatus({ edge_available: false })}
      />,
    );
    await user.click(screen.getByTestId("update-channel-trigger"));
    expect(
      await screen.findByRole("option", { name: "Beta channel" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("option", { name: "Edge channel" }),
    ).not.toBeInTheDocument();
  });

  it("offers the edge channel when the box allows it", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const onChangeChannel = vi.fn();
    renderWithProviders(
      <UpdateCard
        {...baseProps()}
        status={makeStatus({ edge_available: true })}
        onChangeChannel={onChangeChannel}
      />,
    );
    await user.click(screen.getByTestId("update-channel-trigger"));
    await user.click(
      await screen.findByRole("option", { name: "Edge channel" }),
    );
    expect(onChangeChannel).toHaveBeenCalledWith("edge");
  });

  it("shows the rollback in-progress label", () => {
    renderWithProviders(
      <UpdateCard
        {...baseProps()}
        status={makeStatus({ rollback_available: true })}
        isRollingBack
      />,
    );
    expect(screen.getByText("Rolling back…")).toBeInTheDocument();
  });
});
