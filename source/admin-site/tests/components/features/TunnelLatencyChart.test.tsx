import type { ReactNode } from "react";
import { screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TunnelLatencyChart } from "@/components/features/TunnelLatencyChart";
import { renderWithProviders } from "../../test-utils";
import type { TunnelStatsData } from "@wardnet/web";

vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    unobserve() {}
    disconnect() {}
  },
);

// Recharts renders nothing in jsdom (0-size ResponsiveContainer), so the
// axis/tooltip/legend formatter callbacks never fire. Replace the pieces the
// chart uses with tiny stubs that render children and invoke each formatter
// once, so the formatting logic is exercised for coverage.
vi.mock("recharts", () => ({
  ResponsiveContainer: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  LineChart: ({ children }: { children: ReactNode }) => (
    <div data-testid="line-chart">{children}</div>
  ),
  CartesianGrid: () => null,
  ReferenceArea: () => null,
  Line: () => null,
  Legend: ({ formatter }: { formatter?: () => ReactNode }) => (
    <div data-testid="legend">{formatter ? formatter() : null}</div>
  ),
  XAxis: ({ tickFormatter }: { tickFormatter?: (v: number) => ReactNode }) => (
    <div data-testid="xaxis">
      {tickFormatter ? tickFormatter(1_700_000_000_000) : null}
    </div>
  ),
  YAxis: ({ tickFormatter }: { tickFormatter?: (v: number) => ReactNode }) => (
    <div data-testid="yaxis">{tickFormatter ? tickFormatter(42) : null}</div>
  ),
  Tooltip: ({
    labelFormatter,
    formatter,
  }: {
    labelFormatter?: (v: unknown) => ReactNode;
    formatter?: (v: unknown) => ReactNode;
  }) => (
    <div data-testid="tooltip">
      {labelFormatter ? labelFormatter(1_700_000_000_000) : null}
      {formatter ? String(formatter(12.3456)) : null}
      {formatter ? String(formatter(null)) : null}
      {formatter ? String(formatter("n/a")) : null}
    </div>
  ),
}));

const base = {
  range: "24h" as const,
  zoom: null,
  onZoomChange: vi.fn(),
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("TunnelLatencyChart", () => {
  it("renders the error state", () => {
    renderWithProviders(
      <TunnelLatencyChart
        data={undefined}
        isLoading={false}
        isError
        {...base}
      />,
    );
    expect(
      screen.getByText("Failed to load latency history."),
    ).toBeInTheDocument();
  });

  it("renders the loading state", () => {
    renderWithProviders(
      <TunnelLatencyChart
        data={undefined}
        isLoading
        isError={false}
        {...base}
      />,
    );
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("renders the empty state when there are no points", () => {
    const data: TunnelStatsData = { points: [], bucketSecs: 60, range: "24h" };
    renderWithProviders(
      <TunnelLatencyChart
        data={data}
        isLoading={false}
        isError={false}
        {...base}
      />,
    );
    expect(screen.getByText(/No latency samples yet/)).toBeInTheDocument();
  });

  it("renders the chart and exercises the time-of-day tick formatter", () => {
    const data: TunnelStatsData = {
      points: [
        { ts: 1_700_000_000_000, bytesTx: 0, bytesRx: 0, rttMs: 12 },
        { ts: 1_700_000_060_000, bytesTx: 0, bytesRx: 0, rttMs: null },
      ],
      bucketSecs: 60,
      range: "24h",
    };
    renderWithProviders(
      <TunnelLatencyChart
        data={data}
        isLoading={false}
        isError={false}
        {...base}
      />,
    );
    expect(screen.getByText("Latency")).toBeInTheDocument();
    expect(screen.getByTestId("line-chart")).toBeInTheDocument();
    // YAxis stub renders "42 ms" through the ms tick formatter.
    expect(screen.getByText("42 ms")).toBeInTheDocument();
  });

  it("uses the date tick formatter for the 12-month range with a zoom window", () => {
    const data: TunnelStatsData = {
      points: [{ ts: 1_700_000_000_000, bytesTx: 0, bytesRx: 0, rttMs: 5 }],
      bucketSecs: 3600,
      range: "12mo",
    };
    renderWithProviders(
      <TunnelLatencyChart
        data={data}
        isLoading={false}
        isError={false}
        range="12mo"
        zoom={{ start: 1_700_000_000_000, end: 1_700_000_060_000 }}
        onZoomChange={vi.fn()}
      />,
    );
    expect(screen.getByTestId("line-chart")).toBeInTheDocument();
  });
});
