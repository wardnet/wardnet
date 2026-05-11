import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Logo } from "@/components/compound/Logo";

describe("Logo", () => {
  it("renders with default size", () => {
    render(<Logo />);
    const img = screen.getByAltText("Wardnet");
    expect(img).toBeInTheDocument();
    expect(img).toHaveAttribute("width", "32");
    expect(img).toHaveAttribute("height", "32");
  });

  it("renders with custom size", () => {
    render(<Logo size={80} />);
    const img = screen.getByAltText("Wardnet");
    expect(img).toHaveAttribute("width", "80");
    expect(img).toHaveAttribute("height", "80");
  });

  it("applies className", () => {
    render(<Logo className="mt-4" />);
    const img = screen.getByAltText("Wardnet");
    expect(img.className).toContain("mt-4");
  });
});
