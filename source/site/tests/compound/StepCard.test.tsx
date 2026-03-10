import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StepCard } from "@/components/compound/StepCard";

describe("StepCard", () => {
  it("renders step number, title, and description", () => {
    render(<StepCard step={1} title="First Step" description="Do this first." />);

    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("First Step")).toBeInTheDocument();
    expect(screen.getByText("Do this first.")).toBeInTheDocument();
  });

  it("applies className", () => {
    const { container } = render(
      <StepCard step={2} title="Second" description="Desc" className="extra" />,
    );

    expect(container.firstChild).toHaveClass("extra");
  });
});
