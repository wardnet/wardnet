import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
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

  it("renders a react-router Link to an internal href", () => {
    render(
      <MemoryRouter>
        <FeatureCard
          icon={<span>IC</span>}
          title="Network Zones"
          description="Desc"
          href="/docs/network-zones"
        />
      </MemoryRouter>,
    );

    expect(screen.getByRole("link", { name: /network zones/i })).toHaveAttribute(
      "href",
      "/docs/network-zones",
    );
  });

  it("renders a plain anchor for an absolute href", () => {
    render(
      <FeatureCard
        icon={<span>IC</span>}
        title="External"
        description="Desc"
        href="https://example.com/thing"
      />,
    );

    expect(screen.getByRole("link", { name: /external/i })).toHaveAttribute(
      "href",
      "https://example.com/thing",
    );
  });

  it("renders a Premium tag when premium is set", () => {
    render(
      <MemoryRouter>
        <FeatureCard
          icon={<span>IC</span>}
          title="Remote access"
          description="Desc"
          href="/docs/remote-access"
          premium
        />
      </MemoryRouter>,
    );

    expect(screen.getByText("Premium")).toBeInTheDocument();
  });

  it("renders a non-clickable div when no href is provided", () => {
    render(<FeatureCard icon={<span>IC</span>} title="Static" description="Desc" />);
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });
});
