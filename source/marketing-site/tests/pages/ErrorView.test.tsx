import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { ErrorView } from "@/pages/ErrorView";

describe("ErrorView", () => {
  it("renders the message when provided", () => {
    render(<ErrorView message="bundle exploded" />);
    expect(screen.getByText("Something broke on our end")).toBeInTheDocument();
    expect(screen.getByText("bundle exploded")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /report issue/i })).toHaveAttribute(
      "href",
      "https://github.com/wardnet/wardnet/issues/new",
    );
  });

  it("omits the message block when none is given", () => {
    render(<ErrorView />);
    expect(screen.queryByText("bundle exploded")).not.toBeInTheDocument();
  });

  it("calls onRetry when Try again is clicked", async () => {
    const onRetry = vi.fn();
    render(<ErrorView onRetry={onRetry} />);
    await userEvent.click(screen.getByRole("button", { name: /try again/i }));
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it("falls back to window.location.reload when no onRetry is provided", async () => {
    const reload = vi.fn();
    // jsdom's location.reload is a non-writable getter; replace the whole
    // descriptor for this test so we can assert on the call.
    const original = Object.getOwnPropertyDescriptor(window, "location");
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { ...window.location, reload },
    });
    try {
      render(<ErrorView />);
      await userEvent.click(screen.getByRole("button", { name: /try again/i }));
      expect(reload).toHaveBeenCalledOnce();
    } finally {
      if (original) Object.defineProperty(window, "location", original);
    }
  });
});
