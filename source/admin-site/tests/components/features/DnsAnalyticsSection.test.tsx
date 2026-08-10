import { screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DnsAnalyticsSection } from "@/components/features/DnsAnalyticsSection";
import { renderWithProviders } from "../../test-utils";
import type { DevicePoint, DnsPeriodComparison } from "@wardnet/web";
import type { Device, StatsTopResponse } from "@wardnet/js";

// recharts needs a real container size and ResizeObserver under jsdom.
vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    unobserve() {}
    disconnect() {}
  },
);

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

const onSelectDevice = vi.fn();

function renderSection(
  opts: {
    range?: "1h" | "12h" | "24h" | "7d" | "12mo";
    devices?: { id: string; name?: string; device_type?: string }[];
    comparison?: unknown;
    trackers?: unknown;
    perDevice?: unknown;
    perDeviceLoading?: boolean;
  } = {},
) {
  const devices = (opts.devices ?? []) as Device[];
  return renderWithProviders(
    <DnsAnalyticsSection
      range={opts.range ?? "24h"}
      comparison={opts.comparison as DnsPeriodComparison | undefined}
      trackers={opts.trackers as StatsTopResponse | undefined}
      devices={devices}
      selectedDeviceId={devices[0]?.id ?? ""}
      onSelectDevice={onSelectDevice}
      deviceSeries={opts.perDevice as DevicePoint[] | undefined}
      deviceSeriesLoading={opts.perDeviceLoading ?? false}
    />,
  );
}

beforeEach(() => vi.clearAllMocks());

describe("DnsAnalyticsSection", () => {
  it("renders period-over-period comparison with signed deltas", () => {
    renderSection({
      range: "7d",
      comparison: {
        current: { total: 100, blocked: 40, blockedPercent: 40 },
        previous: { total: 50, blocked: 10, blockedPercent: 20 },
        totalChangePercent: 100,
        blockedChangePercent: 300,
      },
    });
    // Period noun for 7d.
    expect(screen.getByText("Compared to previous week")).toBeInTheDocument();
    expect(screen.getByTestId("dns-comparison-queries")).toHaveTextContent(
      "100",
    );
    expect(screen.getByTestId("dns-comparison-queries")).toHaveTextContent(
      "100%",
    );
    expect(screen.getByTestId("dns-comparison-blocked")).toHaveTextContent(
      "300%",
    );
  });

  it("renders a null change as 'new' rather than a percentage", () => {
    renderSection({
      comparison: {
        current: { total: 5000, blocked: 200, blockedPercent: 4 },
        previous: { total: 0, blocked: 0, blockedPercent: 0 },
        totalChangePercent: null,
        blockedChangePercent: null,
        previousPartial: false,
      },
    });
    expect(screen.getByTestId("dns-comparison-queries")).toHaveTextContent(
      "new",
    );
    expect(screen.getByTestId("dns-comparison-queries")).not.toHaveTextContent(
      "%",
    );
  });

  it("lists top trackers by company", () => {
    renderSection({
      trackers: {
        metric: "dns.blocked.by_tracker",
        entries: [
          { labels: JSON.stringify({ company: "Google" }), total: 120 },
          { labels: JSON.stringify({ company: "Meta" }), total: 33 },
        ],
      },
    });
    const card = screen.getByTestId("dns-top-trackers");
    expect(card).toHaveTextContent("Google");
    expect(card).toHaveTextContent("120 blocks");
    expect(card).toHaveTextContent("Meta");
  });

  it("shows the tracker empty state when nothing recognised was blocked", () => {
    renderSection({ trackers: { metric: "m", entries: [] } });
    expect(
      screen.getByText("No recognised trackers blocked yet."),
    ).toBeInTheDocument();
  });

  it("shows the no-devices state for the per-device chart", () => {
    renderSection({ devices: [] });
    expect(screen.getByText("No devices yet.")).toBeInTheDocument();
    // With no devices there is no device selector.
    expect(
      screen.queryByTestId("dns-per-device-select"),
    ).not.toBeInTheDocument();
  });

  it("renders the per-device selector and chart when a device has data", () => {
    renderSection({
      devices: [{ id: "dev-1", name: "Laptop", device_type: "computer" }],
      perDevice: [
        { ts: "2026-01-01T00:00:00Z", total: 10 },
        { ts: "2026-01-01T01:00:00Z", total: 20 },
      ],
    });
    expect(screen.getByTestId("dns-per-device-select")).toBeInTheDocument();
    expect(screen.getByText("Per-device queries")).toBeInTheDocument();
  });

  it("shows a loading state while the per-device series loads", () => {
    renderSection({
      devices: [{ id: "dev-1", name: "Laptop", device_type: "computer" }],
      perDeviceLoading: true,
    });
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("shows an empty state when the selected device has no queries", () => {
    renderSection({
      devices: [{ id: "dev-1", name: "Laptop", device_type: "computer" }],
      perDevice: [],
    });
    expect(
      screen.getByText("No queries recorded for this device yet."),
    ).toBeInTheDocument();
  });

  it("flags an approximate comparison when the previous period is partial", () => {
    renderSection({
      range: "12mo",
      comparison: {
        current: { total: 100, blocked: 10, blockedPercent: 10 },
        previous: { total: 40, blocked: 5, blockedPercent: 12.5 },
        totalChangePercent: 150,
        blockedChangePercent: 100,
        previousPartial: true,
      },
    });
    // Partial-period note plus the 12mo period noun ("year").
    expect(screen.getByText(/predates stored history/)).toBeInTheDocument();
    expect(screen.getByText("Compared to previous year")).toBeInTheDocument();
  });

  it("phrases the comparison period for each range", () => {
    const cases = [
      ["1h", "Compared to previous hour"],
      ["12h", "Compared to previous 12 hours"],
    ] as const;
    for (const [range, text] of cases) {
      const { unmount } = renderSection({
        range,
        comparison: {
          current: { total: 1, blocked: 0, blockedPercent: 0 },
          previous: { total: 1, blocked: 0, blockedPercent: 0 },
          totalChangePercent: 0,
          blockedChangePercent: 0,
          previousPartial: false,
        },
      });
      expect(screen.getByText(text)).toBeInTheDocument();
      unmount();
    }
  });
});
