import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { describe, expect, it } from "vitest";

import { BlogArticle } from "@/pages/BlogArticle";

function renderAt(path: string) {
  render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/blog/:slug" element={<BlogArticle />} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("BlogArticle", () => {
  it("renders the rendered markdown for a real post slug", () => {
    renderAt("/blog/why-i-built-wardnet");
    expect(screen.queryByText(/post coming soon/i)).not.toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 1 }).length).toBeGreaterThan(0);
  });

  it("renders the coming-soon placeholder for an unknown slug", () => {
    renderAt("/blog/does-not-exist");
    expect(screen.getByText(/post coming soon/i)).toBeInTheDocument();
  });
});
