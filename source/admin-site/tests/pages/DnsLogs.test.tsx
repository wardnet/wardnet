import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { QueryLogEvent } from "@wardnet/js";

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

const { useDevices, useDnsQueryLog } = vi.hoisted(() => ({
  useDevices: vi.fn(),
  useDnsQueryLog: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useDevices, useDnsQueryLog };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({
    title,
    actions,
  }: {
    title: ReactNode;
    actions?: ReactNode;
  }) => (
    <div>
      <h1>{title}</h1>
      {actions}
    </div>
  ),
}));
vi.mock("@/components/compound/DeviceSelect", () => ({
  DeviceSelect: ({
    onChange,
    devices,
  }: {
    onChange: (value: string) => void;
    devices: unknown[];
  }) => (
    <button onClick={() => onChange("10.232.1.10")}>
      device-pick:{devices.length}
    </button>
  ),
}));

import DnsLogs from "@/pages/DnsLogs";
import { useDnsLogStore } from "@/stores/dnsLogStore";
import { makeDevice } from "../test-utils";
import { renderWithProviders } from "../test-utils";

const liveEvent = (over: Record<string, unknown> = {}): QueryLogEvent =>
  ({
    timestamp: "2026-07-01T10:00:00Z",
    client_ip: "10.232.1.10",
    domain: "ads.example.com",
    query_type: "A",
    result: "blocked",
    latency_ms: 3.2,
    ...over,
  }) as unknown as QueryLogEvent;

beforeEach(() => {
  vi.clearAllMocks();
  useDnsLogStore.setState({
    entries: [],
    connected: false,
    paused: false,
    skipped: 0,
    filter: { domain: "", client_ip: "", results: [] },
  });
  useDevices.mockReturnValue({
    data: {
      devices: [
        makeDevice({ id: "d1", name: "Laptop", last_ip: "10.232.1.10" }),
        makeDevice({
          id: "d2",
          name: null,
          hostname: null,
          last_ip: undefined,
        }),
        // Duplicate last_ip is de-duped out of the filter list.
        makeDevice({ id: "d3", name: "Dup", last_ip: "10.232.1.10" }),
      ],
    },
  });
  useDnsQueryLog.mockReturnValue({
    data: { entries: [], total: 0 },
    isLoading: false,
  });
});

describe("DnsLogs", () => {
  it("renders the header with an offline live-tail control", () => {
    renderWithProviders(<DnsLogs />);
    expect(
      screen.getByRole("heading", { name: "DNS query log" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Live tail (offline)")).toBeInTheDocument();
    // Only one device row remains after de-duping on last_ip.
    expect(screen.getByText("device-pick:1")).toBeInTheDocument();
  });

  it("renders live rows with resolved device names and result badges", () => {
    useDnsLogStore.setState({
      connected: true,
      entries: [
        liveEvent(),
        liveEvent({
          client_ip: "9.9.9.9",
          domain: "unknown.test",
          result: "blocked_skipped",
        }),
      ],
    });
    renderWithProviders(<DnsLogs />);
    expect(screen.queryByText("(offline)")).not.toBeInTheDocument();
    expect(screen.getByText("Laptop")).toBeInTheDocument();
    expect(screen.getByText("unknown.test")).toBeInTheDocument();
    expect(screen.getByText("blocked (skipped)")).toBeInTheDocument();
  });

  it("filters live rows by the domain search box", async () => {
    useDnsLogStore.setState({
      entries: [
        liveEvent({ domain: "keep.example.com" }),
        liveEvent({ domain: "drop.other.net" }),
      ],
    });
    const user = userEvent.setup();
    renderWithProviders(<DnsLogs />);
    await user.type(screen.getByPlaceholderText("Search domain…"), "keep");
    expect(screen.getByText("keep.example.com")).toBeInTheDocument();
    expect(screen.queryByText("drop.other.net")).not.toBeInTheDocument();
  });

  it("narrows the live tail when a device is picked", async () => {
    useDnsLogStore.setState({
      entries: [
        liveEvent({ domain: "mine.example.com", client_ip: "10.232.1.10" }),
        liveEvent({ domain: "theirs.example.com", client_ip: "9.9.9.9" }),
      ],
    });
    const user = userEvent.setup();
    renderWithProviders(<DnsLogs />);
    await user.click(screen.getByText(/device-pick/));
    expect(screen.getByText("mine.example.com")).toBeInTheDocument();
    expect(screen.queryByText("theirs.example.com")).not.toBeInTheDocument();
  });

  it("filters the live tail by result via the select", async () => {
    useDnsLogStore.setState({
      entries: [
        liveEvent({ domain: "blocked.example.com", result: "blocked" }),
        liveEvent({ domain: "fwd.example.com", result: "forwarded" }),
      ],
    });
    const user = userEvent.setup();
    renderWithProviders(<DnsLogs />);
    await user.click(screen.getByTestId("dns-log-result-filter"));
    await user.click(screen.getByRole("option", { name: "Forwarded" }));
    expect(screen.getByText("fwd.example.com")).toBeInTheDocument();
    expect(screen.queryByText("blocked.example.com")).not.toBeInTheDocument();
  });

  it("switches to persisted history with pagination when live tail is off", async () => {
    useDnsQueryLog.mockReturnValue({
      data: {
        entries: [
          {
            timestamp: "2026-07-01T09:00:00",
            client_ip: "10.232.1.10",
            domain: "history.example.com",
            query_type: "AAAA",
            result: "cache_hit",
            latency_ms: 0.5,
          },
        ],
        total: 120,
      },
      isLoading: false,
    });
    const user = userEvent.setup();
    renderWithProviders(<DnsLogs />);
    await user.click(screen.getByTestId("live-tail"));

    expect(screen.getByText("history.example.com")).toBeInTheDocument();
    expect(screen.getByText(/120 entries · page 1/)).toBeInTheDocument();

    const prev = screen.getByRole("button", { name: "Previous" });
    const next = screen.getByRole("button", { name: "Next" });
    expect(prev).toBeDisabled();
    await user.click(next);
    expect(screen.getByText(/page 2/)).toBeInTheDocument();
    await user.click(prev);
    expect(screen.getByText(/page 1/)).toBeInTheDocument();
  });

  it("shows the loading empty message for persisted history", async () => {
    useDnsQueryLog.mockReturnValue({ data: undefined, isLoading: true });
    const user = userEvent.setup();
    renderWithProviders(<DnsLogs />);
    await user.click(screen.getByTestId("live-tail"));
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("shows the no-match message when there are no rows", () => {
    renderWithProviders(<DnsLogs />);
    expect(
      screen.getByText("No DNS queries match this filter yet."),
    ).toBeInTheDocument();
  });

  it("renders the raw client IP when no device matches", () => {
    useDnsLogStore.setState({
      entries: [liveEvent({ client_ip: "9.9.9.9", domain: "x.test" })],
    });
    renderWithProviders(<DnsLogs />);
    const row = screen.getByText("x.test").closest("tr")!;
    expect(within(row).getByText("9.9.9.9")).toBeInTheDocument();
  });
});
