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
} = vi.hoisted(() => ({
  useDnsStatus: vi.fn(),
  useDnsConfig: vi.fn(),
  useToggleDns: vi.fn(),
  useFlushDnsCache: vi.fn(),
  useUpdateDnsConfig: vi.fn(),
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
  DnsStatsSection: ({ range }: { range: string }) => <div>stats:{range}</div>,
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
