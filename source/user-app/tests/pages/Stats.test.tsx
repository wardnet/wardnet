import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import Stats from "../../src/pages/Stats";
import { todayLocal } from "../../src/lib/dnsDb";
import type { DnsStatsData } from "../../src/hooks/useDnsStats";
import { makeDevice, renderWithProviders } from "../test-utils";

const { useMyDevice, useCreateRuleRequest } = vi.hoisted(() => ({
  useMyDevice: vi.fn(),
  useCreateRuleRequest: vi.fn(),
}));
const { useDnsStats } = vi.hoisted(() => ({ useDnsStats: vi.fn() }));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useMyDevice, useCreateRuleRequest };
});

vi.mock("@/hooks/useDnsStats", () => ({ useDnsStats }));

function statsData(overrides: Partial<DnsStatsData> = {}): DnsStatsData {
  const today = todayLocal();
  return {
    headline: { total: 10, blocked: 4, allowed: 6 },
    topBlocked: [{ domain: "bad.com", count: 4 }],
    topQueried: [{ domain: "good.com", count: 6 }],
    recent: [
      { id: 2, domain: "bad.com", status: "blocked", captured_at: new Date().toISOString() },
      { id: 1, domain: "good.com", status: "forwarded", captured_at: new Date(Date.now() - 3600_000).toISOString() },
    ],
    trend: [
      { date: "2026-06-30", blocked: 1, allowed: 2 },
      { date: today, blocked: 4, allowed: 6 },
    ],
    hasData: true,
    loading: false,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useCreateRuleRequest.mockReturnValue({ mutate: vi.fn(), isPending: false });
  useDnsStats.mockReturnValue(statsData());
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("Stats page", () => {
  it("shows a loading state", () => {
    useMyDevice.mockReturnValue({ data: undefined, isLoading: true });
    renderWithProviders(<Stats />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("shows the not-detected fallback", () => {
    useMyDevice.mockReturnValue({ data: { device: null }, isLoading: false });
    renderWithProviders(<Stats />);
    expect(screen.getByTestId("stats-no-device")).toBeInTheDocument();
  });

  it("shows the capture-off prompt with a settings link", () => {
    useMyDevice.mockReturnValue({
      data: { device: makeDevice({ dns_capture_enabled: false }) },
      isLoading: false,
    });
    renderWithProviders(<Stats />);
    expect(screen.getByTestId("stats-capture-off")).toBeInTheDocument();
    expect(screen.getByTestId("stats-settings-link")).toHaveAttribute(
      "href",
      "/settings",
    );
  });

  it("shows the waiting state when capture is on but there is no data yet", () => {
    useMyDevice.mockReturnValue({
      data: { device: makeDevice({ dns_capture_enabled: true }) },
      isLoading: false,
    });
    useDnsStats.mockReturnValue(statsData({ hasData: false }));
    renderWithProviders(<Stats />);
    expect(screen.getByTestId("stats-waiting")).toBeInTheDocument();
  });

  it("renders the trend, headline counters, domain lists and activity feed", () => {
    useMyDevice.mockReturnValue({
      data: { device: makeDevice({ dns_capture_enabled: true }) },
      isLoading: false,
    });
    renderWithProviders(<Stats />);

    expect(screen.getByTestId("stats-content")).toBeInTheDocument();
    // Trend header shows the number of days.
    expect(screen.getByText("Last 2 days")).toBeInTheDocument();
    // Today's day scope button is present.
    expect(screen.getByText("Today")).toBeInTheDocument();
    // Headline: blocked pct = round(4/10*100) = 40%.
    expect(screen.getByText("40%")).toBeInTheDocument();
    // Domain lists (bad.com/good.com also appear in the activity feed).
    expect(screen.getAllByText("bad.com").length).toBeGreaterThan(0);
    expect(screen.getAllByText("good.com").length).toBeGreaterThan(0);
    // Activity feed pills ("Blocked" also appears as a StatTile label).
    expect(screen.getAllByText("Blocked").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Allowed").length).toBeGreaterThan(0);
  });

  it("opens the request modal from a top-blocked domain (defaults to allow)", async () => {
    useMyDevice.mockReturnValue({
      data: { device: makeDevice({ dns_capture_enabled: true }) },
      isLoading: false,
    });
    renderWithProviders(<Stats />);

    const [askBtn] = screen.getAllByLabelText(/Ask admin about bad.com/);
    await userEvent.click(askBtn);
    expect(screen.getByText("Ask your administrator")).toBeInTheDocument();
    // "Allow it" is preselected for a blocked domain.
    expect(screen.getByRole("button", { name: "Allow it" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("switches the selected day when a trend button is tapped", async () => {
    useMyDevice.mockReturnValue({
      data: { device: makeDevice({ dns_capture_enabled: true }) },
      isLoading: false,
    });
    renderWithProviders(<Stats />);
    // Tapping the past day re-queries useDnsStats with that date.
    await userEvent.click(screen.getByText(/Mon|Tue|Wed|Thu|Fri|Sat|Sun/));
    // useDnsStats called again with the older date at some point.
    expect(useDnsStats).toHaveBeenCalledWith("2026-06-30");
  });

  it("shows empty text when there are no domains or activity", () => {
    useMyDevice.mockReturnValue({
      data: { device: makeDevice({ dns_capture_enabled: true }) },
      isLoading: false,
    });
    useDnsStats.mockReturnValue(
      statsData({ topBlocked: [], topQueried: [], recent: [] }),
    );
    renderWithProviders(<Stats />);
    expect(screen.getByText("Nothing blocked on this day.")).toBeInTheDocument();
    expect(screen.getByText("No queries on this day.")).toBeInTheDocument();
    expect(screen.getByText("No recent activity.")).toBeInTheDocument();
  });
});
