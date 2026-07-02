import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import { renderWithProviders } from "../../test-utils";

describe("ConfirmDialog", () => {
  it("renders nothing visible when closed", () => {
    renderWithProviders(
      <ConfirmDialog
        open={false}
        onOpenChange={vi.fn()}
        title="Delete?"
        description="Are you sure"
        onConfirm={vi.fn()}
      />,
    );
    expect(screen.queryByText("Delete?")).not.toBeInTheDocument();
  });

  it("renders title, description and default confirm label when open", () => {
    renderWithProviders(
      <ConfirmDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Delete?"
        description="Are you sure"
        onConfirm={vi.fn()}
      />,
    );
    expect(screen.getByText("Delete?")).toBeInTheDocument();
    expect(screen.getByText("Are you sure")).toBeInTheDocument();
    expect(screen.getByTestId("confirm-dialog-confirm")).toHaveTextContent(
      "Confirm",
    );
  });

  it("uses a custom confirm label", () => {
    renderWithProviders(
      <ConfirmDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Delete?"
        description="Are you sure"
        confirmLabel="Yes, delete"
        onConfirm={vi.fn()}
      />,
    );
    expect(screen.getByTestId("confirm-dialog-confirm")).toHaveTextContent(
      "Yes, delete",
    );
  });

  it("calls onConfirm when the confirm button is clicked", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    renderWithProviders(
      <ConfirmDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Delete?"
        description="Are you sure"
        onConfirm={onConfirm}
        destructive={false}
      />,
    );
    await user.click(screen.getByTestId("confirm-dialog-confirm"));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});
