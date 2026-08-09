import { act, screen } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DnsStatsSection } from "@/components/features/DnsStatsSection";
import type { ZoomRange } from "@/hooks/useChartZoom";
import { renderWithProviders } from "../../test-utils";
import type { DnsStatsDashboardData, StatsRange } from "@wardnet/web";

// Drag-to-zoom is driven by useChartZoom, whose recharts mouse plumbing does
// not compute an activeLabel under jsdom. Mock the hook so we can deterministically
// commit a zoom (via the captured onZoomChange) while leaving recharts itself
// real so the chart's tick formatter still runs.
const zoomState = vi.hoisted(() => ({
  commit: null as null | ((z: { start: number; end: number } | null) => void),
}));
vi.mock("@/hooks/useChartZoom", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useChartZoom: ({
      zoom,
      onZoomChange,
    }: {
      zoom: unknown;
      onZoomChange: (z: { start: number; end: number } | null) => void;
    }) => {
      zoomState.commit = onZoomChange;
      return {
        chartProps: {
          onMouseDown() {},
          onMouseMove() {},
          onMouseUp() {},
          onMouseLeave() {},
        },
        previewRange: null,
        isZoomed: zoom != null,
        reset: () => onZoomChange(null),
      };
    },
  };
});

vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    unobserve() {}
    disconnect() {}
  },
);

// Real container size so recharts renders and runs the tick formatter.
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

const fullData = {
  series: [
    { ts: "2026-01-01T00:00:00Z", total: 100, blocked: 40 },
    { ts: "2026-01-01T01:00:00Z", total: 200, blocked: 60 },
  ],
  total: 300,
  blocked: 100,
  blockedPercent: 33.3,
  topDomains: {
    entries: [
      { labels: JSON.stringify({ domain: "ads.example.com" }), total: 42 },
      // Malformed labels exercise the parseLabels catch → falls back to raw.
      { labels: "not-json", total: 7 },
    ],
  },
  topClients: {
    entries: [{ labels: JSON.stringify({ client: "10.0.0.5" }), total: 120 }],
  },
} as unknown as DnsStatsDashboardData;

interface HarnessProps {
  range: StatsRange;
  data?: DnsStatsDashboardData;
  isLoading?: boolean;
  isError?: boolean;
  error?: unknown;
}

/** Stateful harness standing in for the page: owns the committed zoom the
 *  way pages/Dns.tsx does, so zoom commits from the chart re-render the
 *  section with the new window. */
function Harness({
  range,
  data,
  isLoading = false,
  isError = false,
  error = null,
}: HarnessProps) {
  const [zoom, setZoom] = useState<ZoomRange | null>(null);
  return (
    <DnsStatsSection
      range={range}
      zoom={zoom}
      onZoomChange={setZoom}
      data={data}
      isLoading={isLoading}
      isError={isError}
      error={error}
      devices={undefined}
    />
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("DnsStatsSection", () => {
  it("renders an error card with the error message", () => {
    renderWithProviders(
      <Harness range="24h" isError error={new Error("stats down")} />,
    );
    expect(screen.getByText("stats down")).toBeInTheDocument();
  });

  it("renders a generic error card for a non-Error rejection", () => {
    renderWithProviders(<Harness range="24h" isError error="weird" />);
    expect(screen.getByText("Failed to load DNS stats")).toBeInTheDocument();
  });

  it("shows the loading state in the chart area", () => {
    renderWithProviders(<Harness range="1h" isLoading />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
    // Stat cards fall back to em dashes with no data.
    expect(screen.getAllByText("-").length).toBeGreaterThanOrEqual(1);
  });

  it("shows an empty chart message when the series is empty", () => {
    renderWithProviders(
      <Harness range="7d" data={{ ...fullData, series: [] }} />,
    );
    expect(screen.getByText("No data yet.")).toBeInTheDocument();
  });

  it("renders stat cards, chart and top lists from full data", () => {
    renderWithProviders(<Harness range="12h" data={fullData} />);
    expect(screen.getByText("Queries (12h)")).toBeInTheDocument();
    expect(screen.getByText("300")).toBeInTheDocument();
    expect(screen.getByText("33.3%")).toBeInTheDocument();
    // Top blocked domain parsed from labels — appears in both the stat
    // card and the top-domains list.
    expect(
      screen.getAllByText("ads.example.com").length,
    ).toBeGreaterThanOrEqual(1);
    // Malformed-label entry falls back to the raw labels string.
    expect(screen.getByText("not-json")).toBeInTheDocument();
    // Top client parsed from labels.
    expect(screen.getByText("10.0.0.5")).toBeInTheDocument();
  });

  it("renders the chart with a multi-day x-axis (7d range)", () => {
    renderWithProviders(<Harness range="7d" data={fullData} />);
    expect(screen.getByText("Queries over time")).toBeInTheDocument();
  });

  it("renders the chart with a monthly x-axis (12mo range)", () => {
    renderWithProviders(<Harness range="12mo" data={fullData} />);
    expect(screen.getByText("Queries over time")).toBeInTheDocument();
  });

  it("commits a zoom (relabelling cards) then resets it", () => {
    renderWithProviders(<Harness range="24h" data={fullData} />);
    const t0 = new Date("2026-01-01T00:00:00Z").getTime();
    const t1 = new Date("2026-01-01T01:00:00Z").getTime();
    // Commit a 1-hour zoom window through the hook's onZoomChange.
    act(() => zoomState.commit?.({ start: t0, end: t1 }));
    // Cards now carry the zoom-window duration label instead of the range.
    expect(screen.getByText("Queries (1h)")).toBeInTheDocument();
    // Window totals cover both points → 300 queries.
    expect(screen.getByText("300")).toBeInTheDocument();
    // Resetting clears the committed zoom.
    act(() => zoomState.commit?.(null));
    expect(screen.getByText("Queries (24h)")).toBeInTheDocument();
  });

  it("renders top-list empty states when entries are missing", () => {
    renderWithProviders(
      <Harness
        range="12mo"
        data={
          {
            ...fullData,
            topDomains: { entries: [] },
            topClients: { entries: [] },
          } as unknown as DnsStatsDashboardData
        }
      />,
    );
    // Both top lists show the empty message.
    expect(screen.getAllByText("No data yet.").length).toBeGreaterThanOrEqual(
      2,
    );
  });
});
