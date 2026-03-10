import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TechStack } from "@/components/layouts/TechStack";

describe("TechStack", () => {
  it("renders the section title", () => {
    render(<TechStack />);
    expect(screen.getByText("Built with modern tools")).toBeInTheDocument();
  });

  it("renders all technology badges", () => {
    render(<TechStack />);
    expect(screen.getByText("Rust")).toBeInTheDocument();
    expect(screen.getByText("React")).toBeInTheDocument();
    expect(screen.getByText("TypeScript")).toBeInTheDocument();
    expect(screen.getByText("WireGuard")).toBeInTheDocument();
    expect(screen.getByText("SQLite")).toBeInTheDocument();
    expect(screen.getByText("Tailwind CSS")).toBeInTheDocument();
    expect(screen.getByText("Raspberry Pi")).toBeInTheDocument();
  });
});
