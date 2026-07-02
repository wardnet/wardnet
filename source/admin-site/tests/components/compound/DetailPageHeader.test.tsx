import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { DetailPageHeader } from "@/components/compound/DetailPageHeader";
import { renderWithProviders } from "../../test-utils";

describe("DetailPageHeader", () => {
  it("renders the breadcrumb link and item title", () => {
    renderWithProviders(
      <DetailPageHeader
        parentLabel="Tunnels"
        parentTo="/tunnels"
        itemLabel="My tunnel"
      />,
    );
    const crumb = screen.getByRole("link", { name: "Tunnels" });
    expect(crumb).toHaveAttribute("href", "/tunnels");
    // itemLabel appears both in breadcrumb Text and the Heading.
    expect(screen.getAllByText("My tunnel").length).toBeGreaterThanOrEqual(1);
    expect(
      screen.getByRole("heading", { name: "My tunnel" }),
    ).toBeInTheDocument();
  });

  it("renders the optional icon, status and meta", () => {
    renderWithProviders(
      <DetailPageHeader
        parentLabel="Tunnels"
        parentTo="/tunnels"
        itemLabel="My tunnel"
        icon={<span data-testid="icon">i</span>}
        status={<span data-testid="status">up</span>}
        meta={<span>last handshake 2m ago</span>}
      />,
    );
    expect(screen.getByTestId("icon")).toBeInTheDocument();
    expect(screen.getByTestId("status")).toBeInTheDocument();
    expect(screen.getByText("last handshake 2m ago")).toBeInTheDocument();
  });

  it("omits icon and meta when not provided", () => {
    const { container } = renderWithProviders(
      <DetailPageHeader
        parentLabel="Tunnels"
        parentTo="/tunnels"
        itemLabel="My tunnel"
      />,
    );
    // Only the breadcrumb chevron svg should exist, no icon wrapper span.
    expect(container.querySelector("p")).not.toBeInTheDocument();
  });
});
