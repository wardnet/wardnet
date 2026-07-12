import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { Features } from "@/components/layouts/Features";

function renderFeatures() {
  render(
    <MemoryRouter>
      <Features />
    </MemoryRouter>,
  );
}

describe("Features", () => {
  it("renders the section title", () => {
    renderFeatures();
    expect(screen.getByText("Everything you need to protect your network")).toBeInTheDocument();
  });

  it("renders every feature card", () => {
    renderFeatures();
    expect(screen.getByText("Per-device VPN routing")).toBeInTheDocument();
    expect(screen.getByText("WireGuard tunnels")).toBeInTheDocument();
    expect(screen.getByText("DNS ad blocking")).toBeInTheDocument();
    expect(screen.getByText("Network Zones")).toBeInTheDocument();
    expect(screen.getByText("Local DNS")).toBeInTheDocument();
    expect(screen.getByText("Built-in DHCP + device discovery")).toBeInTheDocument();
    expect(screen.getByText("VPN provider integration")).toBeInTheDocument();
    expect(screen.getByText("Backup & restore")).toBeInTheDocument();
    expect(screen.getByText("Remote access & auto-HTTPS")).toBeInTheDocument();
    expect(screen.getByText("Mobile apps")).toBeInTheDocument();
  });

  it("links each card to its docs detail page", () => {
    renderFeatures();
    expect(screen.getByRole("link", { name: /Network Zones/i })).toHaveAttribute(
      "href",
      "/docs/network-zones",
    );
    expect(screen.getByRole("link", { name: /Local DNS/i })).toHaveAttribute(
      "href",
      "/docs/local-dns",
    );
    expect(screen.getByRole("link", { name: /Remote access & auto-HTTPS/i })).toHaveAttribute(
      "href",
      "/docs/remote-access",
    );
    expect(screen.getByRole("link", { name: /Mobile apps/i })).toHaveAttribute(
      "href",
      "/docs/mobile-apps",
    );
  });

  it("tags the Premium cards", () => {
    renderFeatures();
    // Remote access, Personal VPN, and mobile apps are the Premium features.
    expect(screen.getAllByText("Premium")).toHaveLength(3);
  });
});
