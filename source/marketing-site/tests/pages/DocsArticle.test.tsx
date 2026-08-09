import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { describe, expect, it } from "vitest";

import { DocsArticle } from "@/pages/DocsArticle";

function renderAt(path: string) {
  render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/docs/:slug" element={<DocsArticle />} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("DocsArticle", () => {
  it("renders the rendered markdown for a real doc slug", () => {
    // `installation.md` is one of the eager-globbed markdown files; the
    // page should run it through ReactMarkdown rather than render the
    // ComingSoon placeholder.
    renderAt("/docs/installation");
    expect(screen.queryByText(/documentation coming soon/i)).not.toBeInTheDocument();
    // Any rendered <h1> from the markdown is enough to prove the
    // markdown renderer ran.
    expect(screen.getAllByRole("heading", { level: 1 }).length).toBeGreaterThan(0);
  });

  it("renders the uninstall doc, which docs.yml links to as a topic", () => {
    // `content/docs.yml` lists an `uninstall` topic. Nothing in the build
    // cross-checks that a listed slug has a markdown file behind it, and
    // `.md` is neither linted nor prettier-formatted here, so a typo in
    // either place would silently ship a "coming soon" card instead.
    renderAt("/docs/uninstall");
    expect(screen.queryByText(/documentation coming soon/i)).not.toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 1 }).length).toBeGreaterThan(0);
  });

  it("renders the ComingSoon placeholder with title and description for a known topic without a doc file", () => {
    // A slug that exists in `content/docs.yml` topics but doesn't have a
    // matching markdown file falls back to ComingSoon with the topic
    // title + description.
    renderAt("/docs/does-not-exist");
    expect(screen.getByText(/documentation coming soon/i)).toBeInTheDocument();
  });
});
