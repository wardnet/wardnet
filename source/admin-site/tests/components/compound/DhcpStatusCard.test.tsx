import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { DhcpStatusResponse } from "@wardnet/js";
import { DhcpStatusCard } from "@/components/compound/DhcpStatusCard";
import { renderWithProviders } from "../../test-utils";

function makeStatus(
  overrides: Partial<DhcpStatusResponse> = {},
): DhcpStatusResponse {
  return {
    enabled: true,
    running: true,
    active_lease_count: 5,
    pool_total: 10,
    pool_used: 4,
    ...overrides,
  };
}

describe("DhcpStatusCard", () => {
  it("shows the running pill and headline numbers", () => {
    renderWithProviders(
      <DhcpStatusCard
        status={makeStatus()}
        onToggle={vi.fn()}
        isPending={false}
      />,
    );
    expect(screen.getByTestId("dhcp-status-pill")).toHaveTextContent("Running");
    expect(screen.getByText("5")).toBeInTheDocument();
    expect(screen.getByText("10")).toBeInTheDocument();
    expect(screen.getByText("4 / 10")).toBeInTheDocument();
  });

  it("shows the stopped pill when not running", () => {
    renderWithProviders(
      <DhcpStatusCard
        status={makeStatus({ running: false })}
        onToggle={vi.fn()}
        isPending={false}
      />,
    );
    expect(screen.getByTestId("dhcp-status-pill")).toHaveTextContent("Stopped");
  });

  it("treats a zero-size pool as 0% usage without dividing by zero", () => {
    renderWithProviders(
      <DhcpStatusCard
        status={makeStatus({ pool_total: 0, pool_used: 0 })}
        onToggle={vi.fn()}
        isPending={false}
      />,
    );
    expect(screen.getByText("0 / 0")).toBeInTheDocument();
  });

  it("invokes onToggle when the switch is clicked", async () => {
    const user = userEvent.setup();
    const onToggle = vi.fn();
    renderWithProviders(
      <DhcpStatusCard
        status={makeStatus({ enabled: false })}
        onToggle={onToggle}
        isPending={false}
      />,
    );
    await user.click(screen.getByTestId("dhcp-toggle"));
    expect(onToggle).toHaveBeenCalledWith(true);
  });

  it("disables the switch while a mutation is pending", () => {
    renderWithProviders(
      <DhcpStatusCard status={makeStatus()} onToggle={vi.fn()} isPending />,
    );
    expect(screen.getByTestId("dhcp-toggle")).toBeDisabled();
  });
});
