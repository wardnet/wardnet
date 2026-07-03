import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SectionLabel } from "@/components/SectionLabel";

describe("SectionLabel", () => {
  it("renders its children", () => {
    render(<SectionLabel>Daemon</SectionLabel>);
    expect(screen.getByText("Daemon")).toBeInTheDocument();
  });
});
