import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Features } from "@/components/layouts/Features";

describe("Features", () => {
  it("renders the section title", () => {
    render(<Features />);
    expect(screen.getByText("Everything you need to protect your network")).toBeInTheDocument();
  });

  it("renders all six feature cards", () => {
    render(<Features />);
    expect(screen.getByText("Per-device routing")).toBeInTheDocument();
    expect(screen.getByText("WireGuard tunnels")).toBeInTheDocument();
    expect(screen.getByText("DNS ad blocking")).toBeInTheDocument();
    expect(screen.getByText("Built-in DHCP server")).toBeInTheDocument();
    expect(screen.getByText("VPN provider integration")).toBeInTheDocument();
    expect(screen.getByText("Self-service model")).toBeInTheDocument();
  });
});
