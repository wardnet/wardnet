import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.setPointerCapture ??= () => {};
Element.prototype.releasePointerCapture ??= () => {};
Element.prototype.scrollIntoView ??= () => {};
vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    unobserve() {}
    disconnect() {}
  },
);

const {
  useDnsStatus,
  useDnsConfig,
  useToggleDns,
  useFlushDnsCache,
  useUpdateDnsConfig,
  useDevices,
  useDnsStatsDashboard,
  useDnsPeriodComparison,
  useDnsTopTrackers,
  useDnsPerDeviceStats,
} = vi.hoisted(() => ({
  useDnsStatus: vi.fn(),
  useDnsConfig: vi.fn(),
  useToggleDns: vi.fn(),
  useFlushDnsCache: vi.fn(),
  useUpdateDnsConfig: vi.fn(),
  useDevices: vi.fn(),
  useDnsStatsDashboard: vi.fn(),
  useDnsPeriodComparison: vi.fn(),
  useDnsTopTrackers: vi.fn(),
  useDnsPerDeviceStats: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDnsStatus,
    useDnsConfig,
    useToggleDns,
    useFlushDnsCache,
    useUpdateDnsConfig,
    useDevices,
    useDnsStatsDashboard,
    useDnsPeriodComparison,
    useDnsTopTrackers,
    useDnsPerDeviceStats,
  };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: { title: ReactNode }) => <h1>{title}</h1>,
}));
vi.mock("@/components/compound/DashboardUsageBar", () => ({
  DashboardUsageBar: ({ value }: { value: number }) => (
    <div>usage:{Math.round(value)}</div>
  ),
}));
vi.mock("@/components/features/UpstreamServersCard", () => ({
  UpstreamServersCard: ({
    fallbackOnly,
    onModeChange,
    onSelectServer,
  }: {
    fallbackOnly?: boolean;
    onModeChange: (mode: string) => void;
    onSelectServer: (address: string) => void;
  }) => (
    <div>
      <div>upstream:{String(fallbackOnly)}</div>
      <button onClick={() => onModeChange("fastest")}>set-fastest</button>
      <button onClick={() => onSelectServer("1.1.1.1")}>set-single</button>
    </div>
  ),
}));
vi.mock("@/components/features/SecuritySettingsCard", () => ({
  SecuritySettingsCard: () => <div>security-card</div>,
}));
vi.mock("@/components/features/DnsStatsSection", () => ({
  DnsStatsSection: ({
    range,
    zoom,
    onZoomChange,
    data,
  }: {
    range: string;
    zoom: { start: number; end: number } | null;
    onZoomChange: (z: { start: number; end: number } | null) => void;
    data?: { total: number };
  }) => (
    <div>
      <div>stats:{range}</div>
      <div data-testid="stats-zoom">{zoom ? "zoomed" : "full"}</div>
      <div data-testid="stats-total">{data?.total ?? "-"}</div>
      <button onClick={() => onZoomChange({ start: 0, end: 3_600_000 })}>
        commit-zoom
      </button>
    </div>
  ),
}));
vi.mock("@/components/features/DnsAnalyticsSection", () => ({
  DnsAnalyticsSection: ({
    range,
    selectedDeviceId,
    onSelectDevice,
    devices,
  }: {
    range: string;
    selectedDeviceId: string;
    onSelectDevice: (id: string) => void;
    devices: Array<{ id: string }>;
  }) => (
    <div>
      <div>analytics:{range}</div>
      <div data-testid="analytics-selected">{selectedDeviceId || "-"}</div>
      <div data-testid="analytics-devices">{devices.length}</div>
      <button onClick={() => onSelectDevice("dev-2")}>pick-device</button>
    </div>
  ),
}));

import Dns from "@/pages/Dns";
import { renderWithProviders } from "../test-utils";

const toggleMutate = vi.fn();
const flushMutate = vi.fn();
const updateMutate = vi.fn();

function setConfig(over: Record<string, unknown> = {}) {
  useDnsConfig.mockReturnValue({
    data: {
      config: {
        resolution_mode: "forwarding",
        dnssec_enabled: true,
        query_log_enabled: true,
        query_log_retention_days: 7,
        upstream_servers: [],
        ...over,
      },
    },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  useToggleDns.mockReturnValue({ mutate: toggleMutate, isPending: false });
  useFlushDnsCache.mockReturnValue({ mutate: flushMutate, isPending: false });
  useUpdateDnsConfig.mockReturnValue({
    mutate: updateMutate,
    isPending: false,
  });
  useDevices.mockReturnValue({
    data: { devices: [{ id: "dev-1" }, { id: "dev-2" }] },
  });
  useDnsStatsDashboard.mockReturnValue({
    data: { total: 300 },
    isLoading: false,
    isError: false,
    error: null,
  });
  useDnsPeriodComparison.mockReturnValue({ data: undefined });
  useDnsTopTrackers.mockReturnValue({ data: undefined });
  useDnsPerDeviceStats.mockReturnValue({ data: undefined, isLoading: false });
  useDnsStatus.mockReturnValue({
    data: {
      running: true,
      enabled: true,
      cache_size: 50,
      cache_capacity: 100,
      cache_hit_rate: 0.8825,
    },
    isLoading: false,
  });
  setConfig();
});

describe("Dns", () => {
  it("shows the loading card", () => {
    useDnsStatus.mockReturnValue({ data: undefined, isLoading: true });
    useDnsConfig.mockReturnValue({ data: undefined });
    renderWithProviders(<Dns />);
    expect(screen.getByText("Loading DNS status...")).toBeInTheDocument();
  });

  it("renders the populated resolver page", () => {
    renderWithProviders(<Dns />);
    expect(screen.getByRole("heading", { name: "DNS" })).toBeInTheDocument();
    expect(screen.getByTestId("dns-status-pill")).toHaveTextContent("Running");
    expect(screen.getByText("50")).toBeInTheDocument();
    expect(screen.getByText("88.3%")).toBeInTheDocument();
    expect(screen.getByText("usage:50")).toBeInTheDocument();
    expect(screen.getByText("security-card")).toBeInTheDocument();
    expect(screen.getByText("upstream:false")).toBeInTheDocument();
    expect(screen.getByText("stats:24h")).toBeInTheDocument();
  });

  it("toggles DNS and flushes the cache", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Dns />);
    await user.click(screen.getByTestId("dns-toggle"));
    expect(toggleMutate).toHaveBeenCalledWith(false);
    await user.click(screen.getByTestId("dns-flush-cache"));
    expect(flushMutate).toHaveBeenCalled();
  });

  it("toggles the query log retention", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Dns />);
    await user.click(
      screen.getByRole("switch", { name: "Enable query log retention" }),
    );
    expect(updateMutate).toHaveBeenCalledWith({ query_log_enabled: false });
  });

  it("edits and saves the retention days", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Dns />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    const input = screen.getByLabelText("Retention (days)");
    await user.clear(input);
    await user.type(input, "10");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(updateMutate).toHaveBeenCalledWith({ query_log_retention_days: 10 });
  });

  it("cancels a retention edit without saving", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Dns />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByText("7 days")).toBeInTheDocument();
    expect(updateMutate).not.toHaveBeenCalled();
  });

  it("renders the disabled query-log branch", () => {
    setConfig({ query_log_enabled: false });
    renderWithProviders(<Dns />);
    expect(screen.getByText("-")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Edit" }),
    ).not.toBeInTheDocument();
  });

  it("marks upstreams as fallback-only in recursive mode", () => {
    setConfig({ resolution_mode: "recursive", dnssec_enabled: false });
    renderWithProviders(<Dns />);
    expect(screen.getByText("upstream:true")).toBeInTheDocument();
    expect(screen.getByText("Disabled")).toBeInTheDocument();
  });

  it("wires routing mode + single-server changes to updateConfig", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Dns />);
    await user.click(screen.getByText("set-fastest"));
    expect(updateMutate).toHaveBeenCalledWith({
      forwarder_selection_mode: "fastest",
      single_upstream: undefined,
    });
    await user.click(screen.getByText("set-single"));
    expect(updateMutate).toHaveBeenCalledWith({
      forwarder_selection_mode: "single",
      single_upstream: "1.1.1.1",
    });
  });

  it("changes the stats range via the tabs", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Dns />);
    const sevenDay = screen.getByRole("tab", { name: "7d" });
    await user.click(sevenDay);
    expect(screen.getByText("stats:7d")).toBeInTheDocument();
    expect(screen.getByText("analytics:7d")).toBeInTheDocument();
  });

  it("passes the hoisted dashboard data and device list to the sections", () => {
    renderWithProviders(<Dns />);
    expect(screen.getByTestId("stats-total")).toHaveTextContent("300");
    expect(screen.getByTestId("analytics-devices")).toHaveTextContent("2");
    // The per-device selection defaults to the first device.
    expect(screen.getByTestId("analytics-selected")).toHaveTextContent("dev-1");
  });

  it("owns the committed zoom, narrows the dashboard window, and invalidates it on range change", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Dns />);
    expect(screen.getByTestId("stats-zoom")).toHaveTextContent("full");
    await user.click(screen.getByText("commit-zoom"));
    expect(screen.getByTestId("stats-zoom")).toHaveTextContent("zoomed");
    // The dashboard's top-N window now follows the zoom.
    expect(useDnsStatsDashboard).toHaveBeenLastCalledWith(
      "24h",
      expect.objectContaining({ from: expect.any(String) }),
    );
    // Switching range drops the stale zoom rather than applying it to the
    // new window.
    await user.click(screen.getByRole("tab", { name: "7d" }));
    expect(screen.getByTestId("stats-zoom")).toHaveTextContent("full");
    expect(useDnsStatsDashboard).toHaveBeenLastCalledWith("7d", undefined);
  });

  it("switches the per-device series to a user-picked device", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Dns />);
    await user.click(screen.getByText("pick-device"));
    expect(screen.getByTestId("analytics-selected")).toHaveTextContent("dev-2");
    expect(useDnsPerDeviceStats).toHaveBeenLastCalledWith("dev-2", "24h");
  });

  it("shows a Stopped pill when the resolver is down", () => {
    useDnsStatus.mockReturnValue({
      data: {
        running: false,
        enabled: false,
        cache_size: 0,
        cache_capacity: 0,
        cache_hit_rate: 0,
      },
      isLoading: false,
    });
    renderWithProviders(<Dns />);
    expect(screen.getByTestId("dns-status-pill")).toHaveTextContent("Stopped");
    expect(screen.getByText("usage:0")).toBeInTheDocument();
  });
});
