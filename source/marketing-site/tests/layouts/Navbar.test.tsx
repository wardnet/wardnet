import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { describe, expect, it, vi } from "vitest";
import { Navbar } from "@/components/layouts/Navbar";

describe("Navbar", () => {
  it("renders the Wardnet logo lockup", () => {
    render(
      <MemoryRouter>
        <Navbar />
      </MemoryRouter>,
    );
    expect(screen.getByAltText("Wardnet")).toBeInTheDocument();
  });

  it("renders the Documentation link", () => {
    render(
      <MemoryRouter>
        <Navbar />
      </MemoryRouter>,
    );
    expect(screen.getByRole("link", { name: "Documentation" })).toHaveAttribute("href", "/docs");
  });

  it("renders the Premium link", () => {
    render(
      <MemoryRouter>
        <Navbar />
      </MemoryRouter>,
    );
    expect(screen.getByRole("link", { name: "Premium" })).toHaveAttribute("href", "/premium");
  });

  it("renders the Blog link", () => {
    render(
      <MemoryRouter>
        <Navbar />
      </MemoryRouter>,
    );
    expect(screen.getByRole("link", { name: "Blog" })).toHaveAttribute("href", "/blog");
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

  it("links the logo to home when no onLogoClick is provided", () => {
    render(
      <MemoryRouter>
        <Navbar />
      </MemoryRouter>,
    );
    expect(screen.getByRole("link", { name: /wardnet/i })).toHaveAttribute("href", "/");
  });

  it("shows a back button and keeps the logo linked to home when showBack is true", () => {
    render(
      <MemoryRouter>
        <Navbar showBack />
      </MemoryRouter>,
    );
    expect(screen.getByRole("button", { name: /go back/i })).toBeInTheDocument();
    expect(document.querySelector(".lucide-arrow-left")).toBeInTheDocument();
    // The logo stays a plain link to home; it is no longer the back control.
    expect(screen.getByRole("link", { name: /wardnet/i })).toHaveAttribute("href", "/");
  });

  it("does not show a back button by default", () => {
    render(
      <MemoryRouter>
        <Navbar />
      </MemoryRouter>,
    );
    expect(screen.queryByRole("button", { name: /go back/i })).not.toBeInTheDocument();
    expect(document.querySelector(".lucide-arrow-left")).not.toBeInTheDocument();
  });

  it("falls back to the backTo destination when there is no history to pop", async () => {
    // jsdom starts each test with a history length of 1, so the back button
    // takes the fallback branch instead of popping real history.
    render(
      <MemoryRouter initialEntries={["/docs/network-zones"]}>
        <Routes>
          <Route path="/docs/:slug" element={<Navbar showBack backTo="/docs" />} />
          <Route path="/docs" element={<div>Docs index</div>} />
        </Routes>
      </MemoryRouter>,
    );
    await userEvent.click(screen.getByRole("button", { name: /go back/i }));
    expect(screen.getByText("Docs index")).toBeInTheDocument();
  });

  it("pops real browser history when there is history to go back to", async () => {
    // Stub window.history.length directly rather than accumulating real
    // navigations, so this test doesn't depend on (or pollute) history
    // state shared with other tests in this file.
    const descriptor = Object.getOwnPropertyDescriptor(window.history, "length");
    Object.defineProperty(window.history, "length", { value: 2, configurable: true });
    try {
      render(
        <MemoryRouter initialEntries={["/one", "/two"]} initialIndex={1}>
          <Routes>
            <Route path="/one" element={<div>Page one</div>} />
            <Route path="/two" element={<Navbar showBack />} />
          </Routes>
        </MemoryRouter>,
      );
      await userEvent.click(screen.getByRole("button", { name: /go back/i }));
      expect(screen.getByText("Page one")).toBeInTheDocument();
    } finally {
      if (descriptor) Object.defineProperty(window.history, "length", descriptor);
    }
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
