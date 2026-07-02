import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { DashboardStatCard } from "@/components/compound/DashboardStatCard";
import { renderWithProviders } from "../../test-utils";

describe("DashboardStatCard", () => {
  it("renders the title and value", () => {
    renderWithProviders(
      <DashboardStatCard title="Devices" value={12} testId="stat-devices" />,
    );
    expect(screen.getByText("Devices")).toBeInTheDocument();
    expect(screen.getByText("12")).toBeInTheDocument();
  });

  it("renders the subtitle when provided", () => {
    renderWithProviders(
      <DashboardStatCard title="Devices" value={12} subtitle="3 online" />,
    );
    expect(screen.getByText("3 online")).toBeInTheDocument();
  });

  it("renders in error mode when an error is set", () => {
    renderWithProviders(
      <DashboardStatCard
        title="Devices"
        value={0}
        error="Failed to load"
        testId="stat-err"
      />,
    );
    expect(screen.getByTestId("stat-err")).toHaveTextContent("Failed to load");
    // The stat title is not rendered in error mode.
    expect(screen.queryByText("Devices")).not.toBeInTheDocument();
  });

  it("wraps the tile in a link when `to` is provided", () => {
    renderWithProviders(
      <DashboardStatCard title="Devices" value={12} to="/devices" />,
    );
    expect(screen.getByRole("link")).toHaveAttribute("href", "/devices");
  });

  it("renders a plain tile (no link) when `to` is omitted", () => {
    renderWithProviders(<DashboardStatCard title="Devices" value={12} />);
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });
});
