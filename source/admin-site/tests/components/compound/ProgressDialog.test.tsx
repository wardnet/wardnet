import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProgressDialog } from "@/components/compound/ProgressDialog";
import { renderWithProviders } from "../../test-utils";

afterEach(() => {
  vi.useRealTimers();
});

const base = {
  title: "Restarting",
  errorMessage: null,
  successIcon: <span data-testid="success-icon">ok</span>,
  successTitle: "Restarted",
};

describe("ProgressDialog", () => {
  it("renders nothing when closed", () => {
    renderWithProviders(
      <ProgressDialog
        {...base}
        open={false}
        phase="in-progress"
        startedAt={null}
        onDismiss={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("progress-dialog")).not.toBeInTheDocument();
  });

  it("shows spinner, description, and elapsed counter while in progress", () => {
    renderWithProviders(
      <ProgressDialog
        {...base}
        open
        phase="in-progress"
        description="Please wait"
        startedAt={Date.now() - 3000}
        onDismiss={vi.fn()}
        timeoutSeconds={30}
      />,
    );
    expect(screen.getByTestId("progress-title")).toHaveTextContent(
      "Restarting",
    );
    expect(screen.getByText("Please wait")).toBeInTheDocument();
    expect(
      screen.getByText(/Elapsed: 3s \(times out at 30s\)/),
    ).toBeInTheDocument();
    // No dismiss button in the in-progress (non-terminal) phase.
    expect(screen.queryByTestId("progress-dismiss")).not.toBeInTheDocument();
  });

  it("advances the elapsed counter on its interval", () => {
    vi.useFakeTimers();
    const started = Date.now();
    renderWithProviders(
      <ProgressDialog
        {...base}
        open
        phase="in-progress"
        startedAt={started}
        onDismiss={vi.fn()}
      />,
    );
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(screen.getByText(/Elapsed: 2s/)).toBeInTheDocument();
  });

  it("renders the success phase with icon, description, action, and dismiss", async () => {
    const user = userEvent.setup();
    const onDismiss = vi.fn();
    renderWithProviders(
      <ProgressDialog
        {...base}
        open
        phase="success"
        successDescription="All good"
        successAction={<button type="button">Go home</button>}
        startedAt={null}
        onDismiss={onDismiss}
      />,
    );
    expect(screen.getByTestId("progress-title")).toHaveTextContent("Restarted");
    expect(screen.getByTestId("success-icon")).toBeInTheDocument();
    expect(screen.getByText("All good")).toBeInTheDocument();
    expect(screen.getByText("Go home")).toBeInTheDocument();
    await user.click(screen.getByTestId("progress-dismiss"));
    expect(onDismiss).toHaveBeenCalled();
  });

  it("renders the failed phase with an error callout", () => {
    renderWithProviders(
      <ProgressDialog
        {...base}
        open
        phase="failed"
        description="Something failed"
        errorMessage="boom happened"
        startedAt={null}
        onDismiss={vi.fn()}
      />,
    );
    expect(screen.getByTestId("progress-title")).toHaveTextContent(
      "Restarting",
    );
    expect(screen.getByText("Something failed")).toBeInTheDocument();
    expect(screen.getByText("boom happened")).toBeInTheDocument();
    expect(screen.getByTestId("progress-dismiss")).toBeInTheDocument();
  });
});
