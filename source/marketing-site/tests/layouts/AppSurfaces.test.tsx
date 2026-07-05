import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AppSurfaces } from "@/components/layouts/AppSurfaces";

describe("AppSurfaces", () => {
  it("renders the section heading", () => {
    render(<AppSurfaces />);
    expect(screen.getByText("Three ways to see your network")).toBeInTheDocument();
  });

  it("renders all three surfaces", () => {
    render(<AppSurfaces />);
    expect(screen.getByText("Desktop admin site")).toBeInTheDocument();
    expect(screen.getByText("User PWA")).toBeInTheDocument();
    expect(screen.getByText("Admin mobile PWA")).toBeInTheDocument();
  });

  it("marks the two PWAs as Premium", () => {
    render(<AppSurfaces />);
    expect(screen.getAllByText("Premium")).toHaveLength(2);
  });

  it("renders a screenshot placeholder for each surface", () => {
    render(<AppSurfaces />);
    expect(screen.getAllByRole("img", { name: /screenshot placeholder/i })).toHaveLength(3);
  });
});
