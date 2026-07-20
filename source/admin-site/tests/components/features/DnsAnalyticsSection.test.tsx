import { screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  useDevices,
  useDnsPeriodComparison,
  useDnsPerDeviceStats,
  useDnsTopTrackers,
} from "@wardnet/web";
import { DnsAnalyticsSection } from "@/components/features/DnsAnalyticsSection";
import { renderWithProviders } from "../../test-utils";

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

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDevices: vi.fn(),
    useDnsPeriodComparison: vi.fn(),
    useDnsPerDeviceStats: vi.fn(),
    useDnsTopTrackers: vi.fn(),
  };
});

const mockDevices = vi.mocked(useDevices);
const mockComparison = vi.mocked(useDnsPeriodComparison);
const mockPerDevice = vi.mocked(useDnsPerDeviceStats);
const mockTrackers = vi.mocked(useDnsTopTrackers);

function setup(opts: {
  devices?: { id: string; name?: string; device_type?: string }[];
  comparison?: unknown;
  trackers?: unknown;
  perDevice?: unknown;
} = {}) {
  mockDevices.mockReturnValue({
    data: { devices: opts.devices ?? [] },
  } as never);
  mockComparison.mockReturnValue({
    data: opts.comparison,
    isLoading: false,
    isError: false,
    error: null,
  } as never);
  mockTrackers.mockReturnValue({ data: opts.trackers } as never);
  mockPerDevice.mockReturnValue({
    data: opts.perDevice,
    isLoading: false,
    isError: false,
    error: null,
  } as never);
}

beforeEach(() => vi.clearAllMocks());

describe("DnsAnalyticsSection", () => {
  it("renders period-over-period comparison with signed deltas", () => {
    setup({
      comparison: {
        current: { total: 100, blocked: 40, blockedPercent: 40 },
        previous: { total: 50, blocked: 10, blockedPercent: 20 },
        totalChangePercent: 100,
        blockedChangePercent: 300,
      },
    });
    renderWithProviders(<DnsAnalyticsSection range="7d" />);
    // Period noun for 7d.
    expect(
      screen.getByText("Compared to previous week"),
    ).toBeInTheDocument();
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

  it("lists top trackers by company", () => {
    setup({
      trackers: {
        metric: "dns.blocked.by_tracker",
        entries: [
          { labels: JSON.stringify({ company: "Google" }), total: 120 },
          { labels: JSON.stringify({ company: "Meta" }), total: 33 },
        ],
      },
    });
    renderWithProviders(<DnsAnalyticsSection range="24h" />);
    const card = screen.getByTestId("dns-top-trackers");
    expect(card).toHaveTextContent("Google");
    expect(card).toHaveTextContent("120 blocks");
    expect(card).toHaveTextContent("Meta");
  });

  it("shows the tracker empty state when nothing recognised was blocked", () => {
    setup({ trackers: { metric: "m", entries: [] } });
    renderWithProviders(<DnsAnalyticsSection range="24h" />);
    expect(
      screen.getByText("No recognised trackers blocked yet."),
    ).toBeInTheDocument();
  });

  it("shows the no-devices state for the per-device chart", () => {
    setup({ devices: [] });
    renderWithProviders(<DnsAnalyticsSection range="24h" />);
    expect(screen.getByText("No devices yet.")).toBeInTheDocument();
    // With no devices there is no device selector.
    expect(
      screen.queryByTestId("dns-per-device-select"),
    ).not.toBeInTheDocument();
  });

  it("renders the per-device selector and chart when a device has data", () => {
    setup({
      devices: [{ id: "dev-1", name: "Laptop", device_type: "computer" }],
      perDevice: [
        { ts: "2026-01-01T00:00:00Z", total: 10 },
        { ts: "2026-01-01T01:00:00Z", total: 20 },
      ],
    });
    renderWithProviders(<DnsAnalyticsSection range="24h" />);
    expect(screen.getByTestId("dns-per-device-select")).toBeInTheDocument();
    expect(screen.getByText("Per-device queries")).toBeInTheDocument();
  });
});
