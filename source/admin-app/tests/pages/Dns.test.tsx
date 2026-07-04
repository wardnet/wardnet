import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  useDnsStatus: vi.fn(),
  useDnsConfig: vi.fn(),
  useDashboardDnsStats: vi.fn(),
  useDnsTopBlockedDomains: vi.fn(),
  toggleMutate: vi.fn(),
  updateMutate: vi.fn(),
  flushMutate: vi.fn(),
  ctx: { value: { showingLastKnownState: false } },
}));
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDnsStatus: h.useDnsStatus,
    useDnsConfig: h.useDnsConfig,
    useDashboardDnsStats: h.useDashboardDnsStats,
    useDnsTopBlockedDomains: h.useDnsTopBlockedDomains,
    useToggleDns: () => ({ mutate: h.toggleMutate, isPending: false }),
    useUpdateDnsConfig: () => ({ mutate: h.updateMutate, isPending: false }),
    useFlushDnsCache: () => ({ mutate: h.flushMutate, isPending: false }),
  };
});
vi.mock("@/context/OnlineStatusContext", () => ({
  useOnlineStatusContext: () => h.ctx.value,
}));

import Dns from "@/pages/Dns";
import { renderWithProviders } from "../test-utils";

function loaded() {
  h.useDnsStatus.mockReturnValue({
    isLoading: false,
    data: {
      running: true,
      cache_size: 42,
      cache_capacity: 100,
      cache_hit_rate: 0.9,
    },
  });
  h.useDnsConfig.mockReturnValue({
    isLoading: false,
    data: { config: { enabled: true, dns_filtering_enabled: true } },
  });
  h.useDashboardDnsStats.mockReturnValue({
    isLoading: false,
    data: {
      total: 1000,
      blocked: 80,
      blockedPercent: 8,
      totalSeries: [1, 2],
      blockedSeries: [0, 1],
    },
  });
  h.useDnsTopBlockedDomains.mockReturnValue({
    isLoading: false,
    data: { entries: [{ labels: "ads.example.com", total: 55 }] },
  });
}

describe("Dns page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.ctx.value = { showingLastKnownState: false };
  });

  it("shows a skeleton while any query is loading", () => {
    h.useDnsStatus.mockReturnValue({ isLoading: true, data: undefined });
    h.useDnsConfig.mockReturnValue({ isLoading: true, data: undefined });
    h.useDashboardDnsStats.mockReturnValue({
      isLoading: true,
      data: undefined,
    });
    h.useDnsTopBlockedDomains.mockReturnValue({
      isLoading: true,
      data: undefined,
    });
    const { container } = renderWithProviders(<Dns />);
    expect(container.querySelector(".animate-pulse")).not.toBeNull();
  });

  it("renders the running pill, cache stats and top blocked list", () => {
    loaded();
    renderWithProviders(<Dns />);
    expect(screen.getByTestId("dns-status-pill")).toHaveTextContent("Running");
    expect(screen.getByText("1,000")).toBeInTheDocument();
    expect(screen.getByText("ads.example.com")).toBeInTheDocument();
    expect(screen.getByText("55")).toBeInTheDocument();
  });

  it("shows the empty message when there are no blocked domains", () => {
    loaded();
    h.useDnsTopBlockedDomains.mockReturnValue({
      isLoading: false,
      data: { entries: [] },
    });
    renderWithProviders(<Dns />);
    expect(
      screen.getByText(/No blocked queries in the last 24 hours/),
    ).toBeInTheDocument();
  });

  it("confirms and toggles the DNS server off", async () => {
    loaded();
    renderWithProviders(<Dns />);
    await userEvent.click(screen.getByTestId("dns-toggle"));
    expect(screen.getByText("Disable DNS server?")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("confirm-dialog-confirm"));
    expect(h.toggleMutate).toHaveBeenCalledWith(false);
  });

  it("confirms and toggles DNS filtering off", async () => {
    loaded();
    renderWithProviders(<Dns />);
    await userEvent.click(screen.getByTestId("dns-filter-toggle"));
    expect(screen.getByText("Disable DNS filtering?")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("confirm-dialog-confirm"));
    expect(h.updateMutate).toHaveBeenCalledWith({
      dns_filtering_enabled: false,
    });
  });

  it("flushes the DNS cache", async () => {
    loaded();
    renderWithProviders(<Dns />);
    await userEvent.click(screen.getByTestId("dns-flush-cache"));
    expect(h.flushMutate).toHaveBeenCalledOnce();
  });

  it("shows the stopped pill and enable copy when DNS is disabled", async () => {
    loaded();
    h.useDnsStatus.mockReturnValue({
      isLoading: false,
      data: {
        running: false,
        cache_size: 0,
        cache_capacity: 0,
        cache_hit_rate: 0,
      },
    });
    h.useDnsConfig.mockReturnValue({
      isLoading: false,
      data: { config: { enabled: false, dns_filtering_enabled: false } },
    });
    renderWithProviders(<Dns />);
    expect(screen.getByTestId("dns-status-pill")).toHaveTextContent("Stopped");
    await userEvent.click(screen.getByTestId("dns-toggle"));
    expect(screen.getByText("Enable DNS server?")).toBeInTheDocument();
  });
});
