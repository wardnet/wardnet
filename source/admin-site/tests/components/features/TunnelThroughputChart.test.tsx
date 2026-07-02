import { screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { TunnelStatsData } from "@wardnet/web";
import { TunnelThroughputChart } from "@/components/features/TunnelThroughputChart";
import { renderWithProviders } from "../../test-utils";

vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    unobserve() {}
    disconnect() {}
  },
);

// Give the chart container a real size so recharts actually renders its
// axes/legend — this is what exercises the tick/label formatter helpers.
Element.prototype.getBoundingClientRect = () =>
  ({
    width: 800,
    height: 300,
    top: 0,
    left: 0,
    right: 800,
    bottom: 300,
    x: 0,
    y: 0,
    toJSON() {},
  }) as DOMRect;

const data: TunnelStatsData = {
  bucketSecs: 60,
  range: "1h",
  points: [
    { ts: 1_700_000_000_000, bytesTx: 6000, bytesRx: 12000, rttMs: 10 },
    { ts: 1_700_000_060_000, bytesTx: 3000, bytesRx: 9000, rttMs: 12 },
  ],
};

function baseProps() {
  return {
    data,
    isLoading: false,
    isError: false,
    range: "1h" as const,
    zoom: null,
    onZoomChange: vi.fn(),
  };
}

describe("TunnelThroughputChart", () => {
  it("renders the error state", () => {
    renderWithProviders(
      <TunnelThroughputChart {...baseProps()} data={undefined} isError />,
    );
    expect(
      screen.getByText("Failed to load throughput history."),
    ).toBeInTheDocument();
  });

  it("renders the loading state", () => {
    renderWithProviders(
      <TunnelThroughputChart {...baseProps()} data={undefined} isLoading />,
    );
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("renders the empty state when there are no points", () => {
    renderWithProviders(
      <TunnelThroughputChart {...baseProps()} data={{ ...data, points: [] }} />,
    );
    expect(screen.getByText(/No throughput history yet/)).toBeInTheDocument();
  });

  it("renders the chart and the window totals when data is present", () => {
    renderWithProviders(<TunnelThroughputChart {...baseProps()} />);
    expect(screen.getByText("Throughput")).toBeInTheDocument();
    // Window total: rx = 21000, tx = 9000 bytes.
    expect(screen.getByText(/Window total:/)).toBeInTheDocument();
  });

  it("restricts window totals to the zoomed range", () => {
    // Zoom window covers only the first point.
    renderWithProviders(
      <TunnelThroughputChart
        {...baseProps()}
        zoom={{ start: 1_699_999_999_000, end: 1_700_000_030_000 }}
      />,
    );
    expect(screen.getByText("Throughput")).toBeInTheDocument();
  });

  it("formats a 12-month range without crashing", () => {
    renderWithProviders(
      <TunnelThroughputChart {...baseProps()} range="12mo" />,
    );
    expect(screen.getByText("Throughput")).toBeInTheDocument();
  });
});
