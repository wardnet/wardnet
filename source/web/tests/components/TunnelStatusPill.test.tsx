import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TunnelStatusPill } from "../../src/components/TunnelStatusPill";

describe("TunnelStatusPill", () => {
  it("renders the human label for a status", () => {
    render(<TunnelStatusPill status="up" />);
    expect(screen.getByText("Active")).toBeInTheDocument();
  });
  it("renders the down label", () => {
    render(<TunnelStatusPill status="down" />);
    expect(screen.getByText("Down")).toBeInTheDocument();
  });
});
