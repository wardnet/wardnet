import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "@/App";

describe("App", () => {
  it("renders without crashing", () => {
    render(<App />);
    expect(document.querySelector("section")).toBeInTheDocument();
  });

  it("contains the hero section", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "Wardnet" })).toBeInTheDocument();
  });
});
