import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { Docs } from "@/pages/Docs";

function renderDocs() {
  render(
    <MemoryRouter>
      <Docs />
    </MemoryRouter>,
  );
}

describe("Docs", () => {
  it("renders the Documentation heading", () => {
    renderDocs();
    expect(screen.getByRole("heading", { name: "Documentation" })).toBeInTheDocument();
  });

  it("renders the coming-soon message", () => {
    renderDocs();
    expect(screen.getByText(/documentation is being written/i)).toBeInTheDocument();
  });

  it("renders the recommended section", () => {
    renderDocs();
    expect(screen.getByText("Recommended")).toBeInTheDocument();
    expect(screen.getByText(/get wardnet running/i)).toBeInTheDocument();
  });

  it("renders the all topics section", () => {
    renderDocs();
    expect(screen.getByRole("heading", { name: "All topics" })).toBeInTheDocument();
    expect(screen.getByText("Configuration")).toBeInTheDocument();
    expect(screen.getByText("WireGuard tunnels")).toBeInTheDocument();
    expect(screen.getByText("SDK reference")).toBeInTheDocument();
  });

  it("renders a back button and links the logo to home", () => {
    renderDocs();
    // The back arrow pops real browser history, so it's a button rather
    // than a fixed link. The logo stays a plain link to home.
    expect(screen.getByRole("button", { name: /go back/i })).toBeInTheDocument();
    const homeLink = screen
      .getAllByRole("link")
      .find((el) => el.getAttribute("href") === "/");
    expect(homeLink).toBeDefined();
  });

  it("links each topic card to its doc slug", () => {
    renderDocs();
    const installLink = screen
      .getAllByRole("link")
      .find((el) => el.getAttribute("href") === "/docs/installation");
    expect(installLink).toBeDefined();
  });
});
