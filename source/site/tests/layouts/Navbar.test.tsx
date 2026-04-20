import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it, vi } from "vitest";
import { Navbar } from "@/components/layouts/Navbar";

describe("Navbar", () => {
  it("renders the Wardnet logo and name", () => {
    render(
      <MemoryRouter>
        <Navbar />
      </MemoryRouter>,
    );
    expect(screen.getByText("Wardnet")).toBeInTheDocument();
    expect(screen.getByAltText("Wardnet logo")).toBeInTheDocument();
  });

  it("renders the Documentation link", () => {
    render(
      <MemoryRouter>
        <Navbar />
      </MemoryRouter>,
    );
    expect(screen.getByRole("link", { name: "Documentation" })).toHaveAttribute("href", "/docs");
  });

  it("renders the GitHub icon link", () => {
    render(
      <MemoryRouter>
        <Navbar />
      </MemoryRouter>,
    );
    expect(screen.getByRole("link", { name: "GitHub" })).toHaveAttribute(
      "href",
      "https://github.com/wardnet/wardnet",
    );
  });

  it("renders a link to home when no onLogoClick is provided", () => {
    render(
      <MemoryRouter>
        <Navbar />
      </MemoryRouter>,
    );
    expect(screen.getByRole("link", { name: /wardnet/i })).toHaveAttribute("href", "/");
  });

  it("shows a back arrow and links to content view when showBack is true", () => {
    render(
      <MemoryRouter>
        <Navbar showBack />
      </MemoryRouter>,
    );
    expect(document.querySelector(".lucide-arrow-left")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /wardnet/i })).toHaveAttribute(
      "href",
      "/?view=content",
    );
  });

  it("does not show a back arrow by default", () => {
    render(
      <MemoryRouter>
        <Navbar />
      </MemoryRouter>,
    );
    expect(document.querySelector(".lucide-arrow-left")).not.toBeInTheDocument();
  });

  it("calls onLogoClick when the logo button is clicked", async () => {
    const onLogoClick = vi.fn();
    render(
      <MemoryRouter>
        <Navbar onLogoClick={onLogoClick} />
      </MemoryRouter>,
    );
    await userEvent.click(screen.getByRole("button", { name: /wardnet/i }));
    expect(onLogoClick).toHaveBeenCalledOnce();
  });
});
