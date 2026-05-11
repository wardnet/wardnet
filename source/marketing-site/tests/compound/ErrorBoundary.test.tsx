import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";

import { ErrorBoundary } from "@/components/compound/ErrorBoundary";

function Boom({ message = "kaboom" }: { message?: string }): never {
  throw new Error(message);
}

describe("ErrorBoundary", () => {
  beforeEach(() => {
    vi.spyOn(console, "error").mockImplementation(() => {});
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders children when no error is thrown", () => {
    render(
      <ErrorBoundary>
        <p>healthy</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("healthy")).toBeInTheDocument();
  });

  it("renders the fallback when a descendant throws", () => {
    render(
      <ErrorBoundary>
        <Boom message="render exploded" />
      </ErrorBoundary>,
    );
    expect(screen.getByText("Something broke on our end")).toBeInTheDocument();
    expect(screen.getByText("render exploded")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /report issue/i })).toHaveAttribute(
      "href",
      "https://github.com/wardnet/wardnet/issues/new",
    );
  });

  it("clears the error and re-renders children when Try again is clicked", async () => {
    function Toggle() {
      // Each render reads the (mutable) module-level flag so the retry
      // can land on a healthy subtree without remounting the boundary.
      if (shouldThrow.value) {
        throw new Error("transient");
      }
      return <p>recovered</p>;
    }
    const shouldThrow = { value: true };

    render(
      <ErrorBoundary>
        <Toggle />
      </ErrorBoundary>,
    );
    expect(screen.getByText("Something broke on our end")).toBeInTheDocument();

    shouldThrow.value = false;
    await userEvent.click(screen.getByRole("button", { name: /try again/i }));

    expect(screen.getByText("recovered")).toBeInTheDocument();
  });
});
