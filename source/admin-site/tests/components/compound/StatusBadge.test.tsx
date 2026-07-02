import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { StatusBadge } from "@/components/compound/StatusBadge";
import { renderWithProviders } from "../../test-utils";

describe("StatusBadge", () => {
  it("renders its children for the success tone", () => {
    renderWithProviders(<StatusBadge tone="success">Running</StatusBadge>);
    expect(screen.getByText("Running")).toBeInTheDocument();
  });

  it("renders a leading check icon for success + withIcon", () => {
    const { container } = renderWithProviders(
      <StatusBadge tone="success" withIcon>
        Up to date
      </StatusBadge>,
    );
    expect(container.querySelector("svg")).toBeInTheDocument();
  });

  it("does not render the icon for a non-success tone even with withIcon", () => {
    const { container } = renderWithProviders(
      <StatusBadge tone="danger" withIcon>
        Failed
      </StatusBadge>,
    );
    expect(container.querySelector("svg")).not.toBeInTheDocument();
    expect(screen.getByText("Failed")).toBeInTheDocument();
  });

  it("renders the neutral tone without an icon by default", () => {
    const { container } = renderWithProviders(
      <StatusBadge tone="neutral">Stopped</StatusBadge>,
    );
    expect(container.querySelector("svg")).not.toBeInTheDocument();
    expect(screen.getByText("Stopped")).toBeInTheDocument();
  });
});
