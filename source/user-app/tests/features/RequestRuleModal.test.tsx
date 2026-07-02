import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  RequestRuleModal,
  type RequestTarget,
} from "../../src/features/RequestRuleModal";

const { useCreateRuleRequest } = vi.hoisted(() => ({
  useCreateRuleRequest: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useCreateRuleRequest };
});

const mutate = vi.fn();

function setup(mutateState: Record<string, unknown> = {}) {
  useCreateRuleRequest.mockReturnValue({
    mutate,
    isPending: false,
    ...mutateState,
  });
}

const target: RequestTarget = { domain: "ads.example.com", kind: "block" };

beforeEach(() => {
  vi.clearAllMocks();
  setup();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("RequestRuleModal", () => {
  it("renders nothing visible when target is null", () => {
    render(<RequestRuleModal target={null} onClose={vi.fn()} />);
    expect(
      screen.queryByText("Ask your administrator"),
    ).not.toBeInTheDocument();
  });

  it("shows the targeted domain when open", () => {
    render(<RequestRuleModal target={target} onClose={vi.fn()} />);
    expect(screen.getByText("Ask your administrator")).toBeInTheDocument();
    expect(screen.getByText("ads.example.com")).toBeInTheDocument();
  });

  it("submits the request with the selected kind and trimmed reason", async () => {
    const onClose = vi.fn();
    render(<RequestRuleModal target={target} onClose={onClose} />);

    // Switch to "allow".
    await userEvent.click(screen.getByRole("button", { name: "Allow it" }));
    await userEvent.type(
      screen.getByPlaceholderText(/Add a note/),
      "  please  ",
    );
    await userEvent.click(screen.getByRole("button", { name: "Send request" }));

    expect(mutate).toHaveBeenCalledTimes(1);
    const [payload, opts] = mutate.mock.calls[0];
    expect(payload).toEqual({
      kind: "allow",
      domain: "ads.example.com",
      reason: "please",
    });
    // onSuccess callback closes the sheet.
    (opts as { onSuccess: () => void }).onSuccess();
    expect(onClose).toHaveBeenCalled();
  });

  it("sends a null reason when the note is blank", async () => {
    render(<RequestRuleModal target={target} onClose={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Send request" }));
    expect(mutate.mock.calls[0][0]).toEqual({
      kind: "block",
      domain: "ads.example.com",
      reason: null,
    });
  });

  it("shows a pending label and disables submit while sending", () => {
    setup({ isPending: true });
    render(<RequestRuleModal target={target} onClose={vi.fn()} />);
    const btn = screen.getByRole("button", { name: "Sending…" });
    expect(btn).toBeDisabled();
  });

  it("calls onClose from the Cancel button", async () => {
    const onClose = vi.fn();
    render(<RequestRuleModal target={target} onClose={onClose} />);
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
  });
});
