import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ShutdownProgressDialog } from "@/components/features/ShutdownProgressDialog";
import { renderWithProviders } from "../../test-utils";

const base = {
  startedAt: 1_000,
  errorMessage: null,
  onDismiss: vi.fn(),
};

describe("ShutdownProgressDialog", () => {
  it("renders nothing when closed", () => {
    renderWithProviders(
      <ShutdownProgressDialog {...base} open={false} phase="scheduled" />,
    );
    expect(screen.queryByTestId("progress-dialog")).not.toBeInTheDocument();
  });

  it("shows the shutting-down title while scheduled", () => {
    renderWithProviders(
      <ShutdownProgressDialog {...base} open phase="scheduled" />,
    );
    expect(screen.getByText("Shutting down Wardnet")).toBeInTheDocument();
    expect(
      screen.getByText("Waiting for the daemon to exit."),
    ).toBeInTheDocument();
  });

  it("shows the confirming title while down", () => {
    renderWithProviders(<ShutdownProgressDialog {...base} open phase="down" />);
    expect(screen.getByText("Confirming Wardnet is off")).toBeInTheDocument();
  });

  it("shows the definitive success state when off", () => {
    renderWithProviders(<ShutdownProgressDialog {...base} open phase="off" />);
    expect(screen.getByText("Wardnet is now off")).toBeInTheDocument();
  });

  it("shows the did_not_fire failure and dismisses", async () => {
    const onDismiss = vi.fn();
    renderWithProviders(
      <ShutdownProgressDialog
        {...base}
        onDismiss={onDismiss}
        open
        phase="did_not_fire"
        errorMessage="privileged migration missing"
      />,
    );
    expect(screen.getByText("Shutdown didn't fire")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("progress-dismiss"));
    expect(onDismiss).toHaveBeenCalled();
  });

  it("shows the timeout failure", () => {
    renderWithProviders(
      <ShutdownProgressDialog {...base} open phase="timeout" />,
    );
    expect(
      screen.getByText("Couldn't confirm Wardnet went off"),
    ).toBeInTheDocument();
  });

  it("shows the generic failed title", () => {
    renderWithProviders(
      <ShutdownProgressDialog {...base} open phase="failed" />,
    );
    expect(screen.getByText("Shutdown request failed")).toBeInTheDocument();
  });
});
