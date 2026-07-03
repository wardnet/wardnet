import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { RestartProgressDialog } from "@/components/features/RestartProgressDialog";
import { renderWithProviders } from "../../test-utils";
import type { RestartPhase } from "@wardnet/web";

function render(phase: RestartPhase, over: Record<string, unknown> = {}) {
  const onDismiss = vi.fn();
  const onSignIn = vi.fn();
  renderWithProviders(
    <RestartProgressDialog
      open
      phase={phase}
      startedAt={1000}
      errorMessage={null}
      onDismiss={onDismiss}
      onSignIn={onSignIn}
      {...over}
    />,
  );
  return { onDismiss, onSignIn };
}

describe("RestartProgressDialog", () => {
  it("does not render content when closed", () => {
    render("scheduled", { open: false });
    expect(screen.queryByTestId("progress-title")).not.toBeInTheDocument();
  });

  it("shows the scheduled (in-progress) title", () => {
    render("scheduled");
    expect(screen.getByText("Restarting daemon")).toBeInTheDocument();
    expect(
      screen.getByText("Waiting for the service to exit."),
    ).toBeInTheDocument();
  });

  it("shows the down title", () => {
    render("down");
    expect(
      screen.getByText("Waiting for daemon to come back"),
    ).toBeInTheDocument();
  });

  it("shows the success (ready) state", () => {
    render("ready");
    expect(screen.getByText("Daemon is back online")).toBeInTheDocument();
  });

  it("offers a sign-in CTA in ready_signed_out and calls onSignIn", async () => {
    const user = userEvent.setup();
    const { onSignIn } = render("ready_signed_out");
    expect(
      screen.getByText("Daemon restarted, session expired"),
    ).toBeInTheDocument();
    await user.click(screen.getByTestId("restart-signin"));
    expect(onSignIn).toHaveBeenCalled();
  });

  it("renders the timeout failure title", () => {
    render("timeout");
    expect(screen.getByText("Daemon didn't come back")).toBeInTheDocument();
  });

  it("renders the failed state with an error message", () => {
    render("failed", { errorMessage: "rejected by server" });
    expect(screen.getByText("Restart request failed")).toBeInTheDocument();
    expect(screen.getByText("rejected by server")).toBeInTheDocument();
  });

  it("renders the did_not_fire failure title", () => {
    render("did_not_fire");
    expect(screen.getByText("Restart didn't fire")).toBeInTheDocument();
  });

  it("falls back to the generic title for an unmapped phase", () => {
    render("idle");
    expect(screen.getByText("Restart")).toBeInTheDocument();
  });

  it("dismisses in a terminal phase", async () => {
    const user = userEvent.setup();
    const { onDismiss } = render("failed", { errorMessage: "x" });
    await user.click(screen.getByTestId("progress-dismiss"));
    expect(onDismiss).toHaveBeenCalled();
  });
});
