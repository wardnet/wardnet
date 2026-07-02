import { screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useDnsRecords } from "@wardnet/web";
import { DhcpLanInfoCard } from "@/components/features/DhcpLanInfoCard";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useDnsRecords: vi.fn() };
});

const mockUseDnsRecords = vi.mocked(useDnsRecords);

describe("DhcpLanInfoCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("counts only DHCP-sourced records", () => {
    mockUseDnsRecords.mockReturnValue({
      data: {
        records: [{ source: "dhcp" }, { source: "dhcp" }, { source: "manual" }],
      },
    } as unknown as ReturnType<typeof useDnsRecords>);

    renderWithProviders(<DhcpLanInfoCard />);

    expect(screen.getByText("2 auto-registered")).toBeInTheDocument();
    expect(screen.getByText("DHCP .lan names")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Manage DHCP" })).toHaveAttribute(
      "href",
      "/dhcp",
    );
  });

  it("renders zero when there is no data", () => {
    mockUseDnsRecords.mockReturnValue({
      data: undefined,
    } as unknown as ReturnType<typeof useDnsRecords>);

    renderWithProviders(<DhcpLanInfoCard />);

    expect(screen.getByText("0 auto-registered")).toBeInTheDocument();
  });
});
