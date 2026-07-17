import { render, screen } from "@testing-library/react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { describe, expect, it } from "vitest";

import { MD_COMPONENTS } from "@/lib/markdown-components";

function renderMarkdown(source: string) {
  render(
    <ReactMarkdown remarkPlugins={[remarkGfm]} components={MD_COMPONENTS}>
      {source}
    </ReactMarkdown>,
  );
}

describe("MD_COMPONENTS", () => {
  it("renders headings, paragraphs, and lists", () => {
    renderMarkdown(
      [
        "# Heading 1",
        "## Heading 2",
        "### Heading 3",
        "",
        "A paragraph.",
        "",
        "- one",
        "- two",
        "",
        "1. first",
        "2. second",
      ].join("\n"),
    );
    expect(screen.getByRole("heading", { level: 1, name: "Heading 1" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "Heading 2" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "Heading 3" })).toBeInTheDocument();
    expect(screen.getByText("A paragraph.")).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(4);
  });

  it("renders a link with the accent styling class", () => {
    renderMarkdown("[docs](/docs)");
    const link = screen.getByRole("link", { name: "docs" });
    expect(link).toHaveAttribute("href", "/docs");
    expect(link.className).toContain("text-accent");
  });

  it("renders inline code with the kbd class", () => {
    renderMarkdown("Run `yarn dev` now.");
    const code = screen.getByText("yarn dev");
    expect(code.tagName).toBe("CODE");
    expect(code.className).toBe("kbd");
  });

  it("renders a fenced code block with its language class passed through", () => {
    renderMarkdown(["```bash", "yarn dev", "```"].join("\n"));
    const code = screen.getByText("yarn dev");
    expect(code.tagName).toBe("CODE");
    expect(code.className).toContain("language-bash");
    expect(code.closest("pre")).toHaveClass("logs");
  });

  it("renders a blockquote", () => {
    renderMarkdown("> a quote");
    expect(screen.getByText("a quote").closest("blockquote")).toBeInTheDocument();
  });

  it("renders a table with header and data cells", () => {
    renderMarkdown(["| A | B |", "| --- | --- |", "| 1 | 2 |"].join("\n"));
    expect(screen.getByRole("columnheader", { name: "A" })).toBeInTheDocument();
    expect(screen.getByRole("cell", { name: "1" })).toBeInTheDocument();
  });

  it("renders a horizontal rule", () => {
    const { container } = render(
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={MD_COMPONENTS}>
        {"above\n\n---\n\nbelow"}
      </ReactMarkdown>,
    );
    expect(container.querySelector("hr")).toBeInTheDocument();
  });

  it("renders strong text", () => {
    renderMarkdown("**bold**");
    expect(screen.getByText("bold").tagName).toBe("STRONG");
  });

  it("caps a default image at max-w-2xl and strips the title attribute", () => {
    renderMarkdown("![alt text](/img.png)");
    const img = screen.getByAltText("alt text");
    expect(img.className).toContain("max-w-2xl");
    expect(img).not.toHaveAttribute("title");
  });

  it("renders a wide image with no width cap", () => {
    renderMarkdown('![alt text](/img.png "wide")');
    const img = screen.getByAltText("alt text");
    expect(img.className).not.toContain("max-w-2xl");
    expect(img.className).not.toContain("max-w-sm");
  });

  it("renders a phone image capped at max-w-sm", () => {
    renderMarkdown('![alt text](/img.png "phone")');
    const img = screen.getByAltText("alt text");
    expect(img.className).toContain("max-w-sm");
  });
});
