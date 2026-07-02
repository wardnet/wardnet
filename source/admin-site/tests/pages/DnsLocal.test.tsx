import { screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({
    title,
    description,
  }: {
    title: ReactNode;
    description?: ReactNode;
  }) => (
    <div>
      <h1>{title}</h1>
      <p>{description}</p>
    </div>
  ),
}));
vi.mock("@/components/features/LocalRecordsCard", () => ({
  LocalRecordsCard: () => <div>local-records</div>,
}));
vi.mock("@/components/features/DnsZonesCard", () => ({
  DnsZonesCard: () => <div>dns-zones</div>,
}));
vi.mock("@/components/features/ConditionalForwardingCard", () => ({
  ConditionalForwardingCard: () => <div>conditional-forwarding</div>,
}));
vi.mock("@/components/features/DhcpLanInfoCard", () => ({
  DhcpLanInfoCard: () => <div>dhcp-lan-info</div>,
}));

import DnsLocal from "@/pages/DnsLocal";
import { renderWithProviders } from "../test-utils";

describe("DnsLocal", () => {
  it("renders the header and composes every child card", () => {
    renderWithProviders(<DnsLocal />);
    expect(
      screen.getByRole("heading", { name: "Local DNS" }),
    ).toBeInTheDocument();
    expect(screen.getByText("local-records")).toBeInTheDocument();
    expect(screen.getByText("dns-zones")).toBeInTheDocument();
    expect(screen.getByText("conditional-forwarding")).toBeInTheDocument();
    expect(screen.getByText("dhcp-lan-info")).toBeInTheDocument();
  });
});
