import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TechBadge } from "@/components/compound/TechBadge";

describe("TechBadge", () => {
  it("renders label text", () => {
    render(<TechBadge label="Rust" />);
    expect(screen.getByText("Rust")).toBeInTheDocument();
  });

  it("applies className", () => {
    render(<TechBadge label="React" className="ml-2" />);
    const badge = screen.getByText("React");
    expect(badge.className).toContain("ml-2");
  });
});
