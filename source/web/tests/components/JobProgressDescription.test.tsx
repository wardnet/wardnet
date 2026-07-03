import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { JobProgressDescription } from "../../src/components/JobProgressDescription";

describe("JobProgressDescription", () => {
  it("renders the clamped percentage", () => {
    render(<JobProgressDescription percentage={42} />);
    expect(screen.getByText("42%")).toBeInTheDocument();
  });
  it("clamps values below 0 and above 100", () => {
    const { rerender } = render(<JobProgressDescription percentage={-10} />);
    expect(screen.getByText("0%")).toBeInTheDocument();
    rerender(<JobProgressDescription percentage={150} />);
    expect(screen.getByText("100%")).toBeInTheDocument();
  });
});
