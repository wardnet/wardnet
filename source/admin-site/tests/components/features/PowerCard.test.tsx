import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PowerCard } from "@/components/features/PowerCard";
import { renderWithProviders } from "../../test-utils";

const onReboot = vi.fn();
const onShutdown = vi.fn();
const onRestartDaemon = vi.fn();

function render(busy = false) {
  renderWithProviders(
    <PowerCard
      onReboot={onReboot}
      onShutdown={onShutdown}
      onRestartDaemon={onRestartDaemon}
      busy={busy}
    />,
  );
}

beforeEach(() => vi.clearAllMocks());

describe("PowerCard", () => {
  it("renders the three lifecycle cards", () => {
    render();
    expect(screen.getByRole("button", { name: "Reboot" })).toBeInTheDocument();
    expect(screen.getByTestId("power-restart-daemon")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Shutdown" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/Power Wardnet off/)).toBeInTheDocument();
  });

  it("confirms a reboot", async () => {
    const user = userEvent.setup();
    render();
    await user.click(screen.getByRole("button", { name: "Reboot" }));
    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Reboot" }));
    expect(onReboot).toHaveBeenCalled();
  });

  it("confirms a shutdown", async () => {
    const user = userEvent.setup();
    render();
    await user.click(screen.getByRole("button", { name: "Shutdown" }));
    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Shut down" }));
    expect(onShutdown).toHaveBeenCalled();
  });

  it("confirms a daemon restart", async () => {
    const user = userEvent.setup();
    render();
    await user.click(screen.getByTestId("power-restart-daemon"));
    await screen.findByRole("alertdialog");
    await user.click(screen.getByTestId("power-restart-confirm"));
    expect(onRestartDaemon).toHaveBeenCalled();
  });

  it("cancels without firing the action", async () => {
    const user = userEvent.setup();
    render();
    await user.click(screen.getByRole("button", { name: "Reboot" }));
    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(onReboot).not.toHaveBeenCalled();
  });

  it("disables every trigger while busy", () => {
    render(true);
    expect(screen.getByRole("button", { name: "Reboot" })).toBeDisabled();
    expect(screen.getByTestId("power-restart-daemon")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Shutdown" })).toBeDisabled();
  });
});
