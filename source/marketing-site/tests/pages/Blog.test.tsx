import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { Blog } from "@/pages/Blog";

function renderBlog() {
  render(
    <MemoryRouter>
      <Blog />
    </MemoryRouter>,
  );
}

describe("Blog", () => {
  it("renders the Blog heading", () => {
    renderBlog();
    expect(screen.getByRole("heading", { name: "Blog" })).toBeInTheDocument();
  });

  it("lists the origin-story post", () => {
    renderBlog();
    expect(screen.getByText("My TV can't run a VPN. So I gave it one anyway.")).toBeInTheDocument();
  });

  it("links each post card to its slug", () => {
    renderBlog();
    const link = screen
      .getAllByRole("link")
      .find((el) => el.getAttribute("href") === "/blog/why-i-built-wardnet");
    expect(link).toBeDefined();
  });

  it("renders a back button and links the logo to home", () => {
    renderBlog();
    expect(screen.getByRole("button", { name: /go back/i })).toBeInTheDocument();
    const homeLink = screen.getAllByRole("link").find((el) => el.getAttribute("href") === "/");
    expect(homeLink).toBeDefined();
  });
});
