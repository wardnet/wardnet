import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FeatureCard } from "@/components/compound/FeatureCard";

describe("FeatureCard", () => {
  it("renders icon, title, and description", () => {
    render(
      <FeatureCard
        icon={<span data-testid="icon">IC</span>}
        title="Test Feature"
        description="A test description."
      />,
    );

    expect(screen.getByTestId("icon")).toBeInTheDocument();
    expect(screen.getByText("Test Feature")).toBeInTheDocument();
    expect(screen.getByText("A test description.")).toBeInTheDocument();
  });

  it("applies className", () => {
    const { container } = render(
      <FeatureCard
        icon={<span>IC</span>}
        title="Title"
        description="Desc"
        className="custom-class"
      />,
    );

    expect(container.firstChild).toHaveClass("custom-class");
  });
});
