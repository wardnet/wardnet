import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { UpdateBanner } from "@/components/compound/UpdateBanner";
import { renderWithProviders } from "../../test-utils";

describe("UpdateBanner", () => {
  it("renders nothing when no update is available", () => {
    const { container } = renderWithProviders(
      <UpdateBanner updateAvailable={false} latestVersion="1.2.3" />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when latestVersion is null", () => {
    const { container } = renderWithProviders(
      <UpdateBanner updateAvailable={true} latestVersion={null} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("renders a link to settings with the version when available", () => {
    renderWithProviders(
      <UpdateBanner updateAvailable={true} latestVersion="1.2.3" />,
    );
    const link = screen.getByRole("link", {
      name: /Update available: v1.2.3/,
    });
    expect(link).toHaveAttribute("href", "/settings");
  });
});
