import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { Footer } from "@/components/layouts/Footer";

function renderFooter() {
  render(
    <MemoryRouter>
      <Footer />
    </MemoryRouter>,
  );
}

describe("Footer", () => {
  it("renders the copyright text", () => {
    renderFooter();
    expect(screen.getByText("MIT License. Built with Rust and React.")).toBeInTheDocument();
  });

  it("renders the GitHub link", () => {
    renderFooter();
    const link = screen.getByRole("link", { name: "GitHub" });
    expect(link).toHaveAttribute("href", "https://github.com/pedromvgomes/wardnet");
  });

  it("renders the Releases link", () => {
    renderFooter();
    const link = screen.getByRole("link", { name: "Releases" });
    expect(link).toHaveAttribute("href", "https://github.com/pedromvgomes/wardnet/releases");
  });

  it("renders the Documentation link", () => {
    renderFooter();
    expect(screen.getByRole("link", { name: "Documentation" })).toHaveAttribute("href", "/docs");
  });

  it("renders the Wardnet name", () => {
    renderFooter();
    expect(screen.getByText("Wardnet")).toBeInTheDocument();
  });
});
