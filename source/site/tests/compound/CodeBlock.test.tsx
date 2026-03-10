import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { CodeBlock } from "@/components/compound/CodeBlock";

describe("CodeBlock", () => {
  it("renders code content", () => {
    render(<CodeBlock code="npm install wardnet" />);
    expect(screen.getByText("npm install wardnet")).toBeInTheDocument();
  });

  it("applies className", () => {
    const { container } = render(<CodeBlock code="echo hello" className="mt-4" />);
    expect(container.firstChild).toHaveClass("mt-4");
  });

  it("wraps code in a monospace code element", () => {
    render(<CodeBlock code="test" />);
    const codeEl = screen.getByText("test");
    expect(codeEl.tagName).toBe("CODE");
    expect(codeEl.className).toContain("font-mono");
  });
});
