import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Logo } from "@/components/compound/Logo";

describe("Logo", () => {
  it("renders the lockup at the default height with auto width", () => {
    render(<Logo />);
    const img = screen.getByAltText("Wardnet");
    expect(img).toBeInTheDocument();
    expect(img.style.height).toBe("24px");
    expect(img.style.width).toBe("auto");
  });

  it("renders with a custom height", () => {
    render(<Logo height={80} />);
    const img = screen.getByAltText("Wardnet");
    expect(img.style.height).toBe("80px");
  });

  it("selects a different lockup for the tagline variant", () => {
    const { rerender } = render(<Logo />);
    const plain = screen.getByAltText("Wardnet").getAttribute("src");
    rerender(<Logo tagline />);
    const tagline = screen.getByAltText("Wardnet").getAttribute("src");
    expect(tagline).not.toBe(plain);
  });

  it("selects a different lockup per background variant", () => {
    const { rerender } = render(<Logo variant="dark" />);
    const dark = screen.getByAltText("Wardnet").getAttribute("src");
    rerender(<Logo variant="light" />);
    const light = screen.getByAltText("Wardnet").getAttribute("src");
    expect(light).not.toBe(dark);
  });

  it("applies className", () => {
    render(<Logo className="mt-4" />);
    const img = screen.getByAltText("Wardnet");
    expect(img.className).toContain("mt-4");
  });
});
