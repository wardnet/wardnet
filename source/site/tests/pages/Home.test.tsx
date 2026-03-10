import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { Home } from "@/pages/Home";

function renderHome() {
  render(
    <MemoryRouter>
      <Home />
    </MemoryRouter>,
  );
}

describe("Home", () => {
  it("initially shows the hero", () => {
    renderHome();
    expect(screen.getByRole("heading", { name: "Wardnet" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /scroll to features/i })).toBeInTheDocument();
  });

  it("shows content sections after clicking Explore", async () => {
    renderHome();
    await userEvent.click(screen.getByRole("button", { name: /scroll to features/i }));
    expect(screen.getByText("Everything you need to protect your network")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /scroll to features/i })).not.toBeInTheDocument();
  });
});
