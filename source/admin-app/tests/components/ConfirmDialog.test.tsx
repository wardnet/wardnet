import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConfirmDialog } from "@/components/ConfirmDialog";

describe("ConfirmDialog", () => {
  it("renders title, description and default confirm label when open", () => {
    render(
      <ConfirmDialog
        open
        onOpenChange={vi.fn()}
        onConfirm={vi.fn()}
        title="Disable DNS?"
        description="Everything breaks."
      />,
    );
    expect(screen.getByText("Disable DNS?")).toBeInTheDocument();
    expect(screen.getByText("Everything breaks.")).toBeInTheDocument();
    expect(screen.getByTestId("confirm-dialog-confirm")).toHaveTextContent(
      "Confirm",
    );
  });

  it("fires onConfirm when the confirm button is pressed", async () => {
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog
        open
        onOpenChange={vi.fn()}
        onConfirm={onConfirm}
        title="t"
        description="d"
        confirmLabel="Disable"
      />,
    );
    await userEvent.click(screen.getByTestId("confirm-dialog-confirm"));
    expect(onConfirm).toHaveBeenCalledOnce();
    expect(screen.getByTestId("confirm-dialog-confirm")).toHaveTextContent(
      "Disable",
    );
  });

  it("applies the warn variant styling", () => {
    render(
      <ConfirmDialog
        open
        onOpenChange={vi.fn()}
        onConfirm={vi.fn()}
        title="t"
        description="d"
        variant="warn"
      />,
    );
    expect(screen.getByTestId("confirm-dialog-confirm").className).toContain(
      "bg-warn",
    );
  });

  it("renders nothing while closed", () => {
    render(
      <ConfirmDialog
        open={false}
        onOpenChange={vi.fn()}
        onConfirm={vi.fn()}
        title="hidden"
        description="d"
      />,
    );
    expect(
      screen.queryByTestId("confirm-dialog-confirm"),
    ).not.toBeInTheDocument();
  });
});
