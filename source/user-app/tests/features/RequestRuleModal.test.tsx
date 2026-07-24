import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  RequestRuleModal,
  type RequestTarget,
} from "../../src/features/RequestRuleModal";

const target: RequestTarget = { domain: "ads.example.com", kind: "block" };

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("RequestRuleModal", () => {
  it("renders nothing visible when target is null", () => {
    render(
      <RequestRuleModal
        target={null}
        onClose={vi.fn()}
        onSubmit={vi.fn()}
        isSubmitting={false}
      />,
    );
    expect(
      screen.queryByText("Ask your administrator"),
    ).not.toBeInTheDocument();
  });

  it("shows the targeted domain when open", () => {
    render(
      <RequestRuleModal
        target={target}
        onClose={vi.fn()}
        onSubmit={vi.fn()}
        isSubmitting={false}
      />,
    );
    expect(screen.getByText("Ask your administrator")).toBeInTheDocument();
    expect(screen.getByText("ads.example.com")).toBeInTheDocument();
  });

  it("fires onSubmit with the selected kind and trimmed reason", async () => {
    const onSubmit = vi.fn();
    render(
      <RequestRuleModal
        target={target}
        onClose={vi.fn()}
        onSubmit={onSubmit}
        isSubmitting={false}
      />,
    );

    // Switch to "allow".
    await userEvent.click(screen.getByRole("button", { name: "Allow it" }));
    await userEvent.type(
      screen.getByPlaceholderText(/Add a note/),
      "  please  ",
    );
    await userEvent.click(screen.getByRole("button", { name: "Send request" }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith({
      kind: "allow",
      domain: "ads.example.com",
      reason: "please",
    });
  });

  it("submits a null reason when the note is blank", async () => {
    const onSubmit = vi.fn();
    render(
      <RequestRuleModal
        target={target}
        onClose={vi.fn()}
        onSubmit={onSubmit}
        isSubmitting={false}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Send request" }));
    expect(onSubmit).toHaveBeenCalledWith({
      kind: "block",
      domain: "ads.example.com",
      reason: null,
    });
  });

  it("shows a pending label and disables submit while submitting", () => {
    render(
      <RequestRuleModal
        target={target}
        onClose={vi.fn()}
        onSubmit={vi.fn()}
        isSubmitting
      />,
    );
    const btn = screen.getByRole("button", { name: "Sending…" });
    expect(btn).toBeDisabled();
  });

  it("calls onClose from the Cancel button", async () => {
    const onClose = vi.fn();
    render(
      <RequestRuleModal
        target={target}
        onClose={onClose}
        onSubmit={vi.fn()}
        isSubmitting={false}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
  });
});
