import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { Premium } from "@/pages/Premium";

function renderPremium() {
  render(
    <MemoryRouter>
      <Premium />
    </MemoryRouter>,
  );
}

describe("Premium page", () => {
  it("renders the premium value story", () => {
    renderPremium();
    expect(screen.getByRole("heading", { name: "Premium" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Start free trial" })).toHaveAttribute(
      "href",
      "https://account.wardnet.network",
    );
  });

  it("renders the app-surfaces showcase", () => {
    renderPremium();
    expect(screen.getByText("Three ways to see your network")).toBeInTheDocument();
  });
});
