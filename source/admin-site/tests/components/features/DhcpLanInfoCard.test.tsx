import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DhcpLanInfoCard } from "@/components/features/DhcpLanInfoCard";
import { renderWithProviders } from "../../test-utils";

// biome-ignore lint/security/noSecrets: identifier-shaped string, not a credential — the entropy heuristic misfires on long CamelCase names
describe("DhcpLanInfoCard", () => {
  it("renders the count of DHCP-sourced records", () => {
    renderWithProviders(<DhcpLanInfoCard dhcpRecordCount={2} />);

    expect(screen.getByText("2 auto-registered")).toBeInTheDocument();
    expect(screen.getByText("DHCP .lan names")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Manage DHCP" })).toHaveAttribute(
      "href",
      "/dhcp",
    );
  });

  it("renders zero when there are no DHCP records", () => {
    renderWithProviders(<DhcpLanInfoCard dhcpRecordCount={0} />);

    expect(screen.getByText("0 auto-registered")).toBeInTheDocument();
  });
});
