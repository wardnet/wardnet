import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";

import { TabBar } from "../../src/components/TabBar";
import { renderWithProviders } from "../test-utils";

describe("TabBar", () => {
  it("renders the three tabs as links", () => {
    renderWithProviders(<TabBar />);
    expect(screen.getByRole("link", { name: /Home/ })).toHaveAttribute(
      "href",
      "/",
    );
    expect(screen.getByRole("link", { name: /Stats/ })).toHaveAttribute(
      "href",
      "/stats",
    );
    expect(screen.getByRole("link", { name: /Settings/ })).toHaveAttribute(
      "href",
      "/settings",
    );
  });

  it("marks the active route as current", () => {
    renderWithProviders(<TabBar />, { route: "/stats" });
    expect(screen.getByRole("link", { name: /Stats/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    // Home has `end`, so it is not current on /stats.
    expect(screen.getByRole("link", { name: /Home/ })).not.toHaveAttribute(
      "aria-current",
    );
  });
});
