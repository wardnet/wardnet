import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import type { DhcpStatusResponse } from "@wardnet/js";
import { DhcpSummaryCard } from "@/components/compound/DhcpSummaryCard";
import { renderWithProviders } from "../../test-utils";

function makeStatus(
  overrides: Partial<DhcpStatusResponse> = {},
): DhcpStatusResponse {
  return {
    running: true,
    active_lease_count: 5,
    pool_used: 25,
    pool_total: 100,
    ...overrides,
  } as DhcpStatusResponse;
}

describe("DhcpSummaryCard", () => {
  it("renders nothing when status is undefined", () => {
    const { container } = renderWithProviders(
      <DhcpSummaryCard status={undefined} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("computes the pool percent and shows Running", () => {
    renderWithProviders(<DhcpSummaryCard status={makeStatus()} />);
    expect(screen.getByText(/25% pool used/)).toBeInTheDocument();
    expect(screen.getByText("Running")).toBeInTheDocument();
  });

  it("shows Stopped when not running", () => {
    renderWithProviders(
      <DhcpSummaryCard status={makeStatus({ running: false })} />,
    );
    expect(screen.getByText("Stopped")).toBeInTheDocument();
  });

  it("handles a zero-sized pool without dividing by zero", () => {
    renderWithProviders(
      <DhcpSummaryCard status={makeStatus({ pool_total: 0, pool_used: 0 })} />,
    );
    expect(screen.getByText(/0% pool used/)).toBeInTheDocument();
  });

  it("clamps the displayed percentage to 100 when the pool is oversubscribed", () => {
    // A pool shrunk below its active-lease count must read "100%", matching
    // the bar's own 100% cap — never ">100%".
    renderWithProviders(
      <DhcpSummaryCard
        status={makeStatus({ pool_total: 100, pool_used: 120 })}
      />,
    );
    expect(screen.getByText(/100% pool used/)).toBeInTheDocument();
    expect(screen.queryByText(/120% pool used/)).not.toBeInTheDocument();
  });

  it("wraps the tile in a link when `to` is provided", () => {
    renderWithProviders(<DhcpSummaryCard status={makeStatus()} to="/dhcp" />);
    expect(screen.getByRole("link")).toHaveAttribute("href", "/dhcp");
  });
});
