import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useDevices } = vi.hoisted(() => ({ useDevices: vi.fn() }));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useDevices };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: { title: ReactNode }) => <h1>{title}</h1>,
}));
vi.mock("@/components/compound/DiscoveryPlaceholder", () => ({
  DiscoveryPlaceholder: ({ message }: { message: ReactNode }) => (
    <div data-testid="placeholder">{message}</div>
  ),
}));
vi.mock("@/components/compound/DeviceTable", () => ({
  DeviceTable: ({
    devices,
    onDeviceClick,
    groups,
    activeGroup,
    onGroupChange,
    searchValue,
    onSearchChange,
  }: {
    devices: Array<{ id: string }>;
    onDeviceClick: (id: string) => void;
    groups: Array<{ id: string; count: number }>;
    activeGroup: string;
    onGroupChange: (id: string) => void;
    searchValue: string;
    onSearchChange: (value: string) => void;
  }) => (
    <div data-testid="device-table">
      <span data-testid="count">{devices.length}</span>
      <span data-testid="active-group">{activeGroup}</span>
      <span data-testid="search-value">{searchValue}</span>
      {groups.map((g) => (
        <button key={g.id} onClick={() => onGroupChange(g.id)}>
          {`group-${g.id}-${g.count}`}
        </button>
      ))}
      <input
        aria-label="search"
        value={searchValue}
        onChange={(e) => onSearchChange(e.target.value)}
      />
      {devices.map((d) => (
        <button key={d.id} onClick={() => onDeviceClick(d.id)}>
          {`open-${d.id}`}
        </button>
      ))}
    </div>
  ),
}));

import Devices from "@/pages/Devices";
import { makeDevice, renderWithProviders } from "../test-utils";

beforeEach(() => {
  vi.clearAllMocks();
  useDevices.mockReturnValue({
    data: undefined,
    isLoading: false,
    isError: false,
  });
});

describe("Devices", () => {
  it("shows placeholder while loading", () => {
    useDevices.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    });
    renderWithProviders(<Devices />);
    expect(screen.getByTestId("placeholder")).toBeInTheDocument();
  });

  it("shows placeholder when not errored and empty", () => {
    useDevices.mockReturnValue({
      data: { devices: [] },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<Devices />);
    expect(screen.getByTestId("placeholder")).toBeInTheDocument();
  });

  it("renders the table (not placeholder) when errored even if empty", () => {
    useDevices.mockReturnValue({
      data: { devices: [] },
      isLoading: false,
      isError: true,
    });
    renderWithProviders(<Devices />);
    expect(screen.queryByTestId("placeholder")).not.toBeInTheDocument();
    expect(screen.getByTestId("device-table")).toBeInTheDocument();
  });

  it("renders devices with per-group counts", () => {
    const recent = new Date().toISOString();
    useDevices.mockReturnValue({
      data: {
        devices: [
          makeDevice({ id: "d1", name: "Managed", last_seen: recent }),
          makeDevice({ id: "d2", name: null, last_seen: "not-a-date" }),
          makeDevice({ id: "d3", name: null, last_seen: undefined }),
        ],
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<Devices />);
    expect(screen.getByTestId("count")).toHaveTextContent("3");
    expect(screen.getByText("group-all-3")).toBeInTheDocument();
    expect(screen.getByText("group-managed-1")).toBeInTheDocument();
    expect(screen.getByText("group-unmanaged-2")).toBeInTheDocument();
    expect(screen.getByText("group-recent-1")).toBeInTheDocument();
  });

  it("drops a device out of 'Recently seen' as wall-clock time elapses", () => {
    vi.useFakeTimers();
    try {
      const mount = new Date("2026-07-22T12:00:00Z").getTime();
      vi.setSystemTime(mount);
      // Seen at mount → inside the 1-hour window.
      useDevices.mockReturnValue({
        data: {
          devices: [
            makeDevice({ id: "d1", last_seen: new Date(mount).toISOString() }),
          ],
        },
        isLoading: false,
        isError: false,
      });
      renderWithProviders(<Devices />);
      expect(screen.getByText("group-recent-1")).toBeInTheDocument();

      // Advance real time past the window without remounting; the live
      // reference clock ticks and the device ages out of the bucket.
      act(() => {
        vi.advanceTimersByTime(61 * 60 * 1000);
      });
      expect(screen.getByText("group-recent-0")).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("filters by group, search, and opens a device", async () => {
    const recent = new Date().toISOString();
    useDevices.mockReturnValue({
      data: {
        devices: [
          makeDevice({
            id: "d1",
            name: "Alpha",
            hostname: "alpha",
            last_ip: "10.0.0.1",
            last_seen: recent,
          }),
          makeDevice({
            id: "d2",
            name: null,
            hostname: "beta",
            mac: "11:22:33:44:55:66",
            last_ip: "10.0.0.2",
          }),
        ],
      },
      isLoading: false,
      isError: false,
    });
    const user = userEvent.setup();
    renderWithProviders(<Devices />);

    // Group filter → managed.
    await user.click(screen.getByText("group-managed-1"));
    expect(screen.getByTestId("active-group")).toHaveTextContent("managed");
    expect(screen.getByTestId("count")).toHaveTextContent("1");

    // Group filter → unmanaged.
    await user.click(screen.getByText("group-unmanaged-1"));
    expect(screen.getByTestId("count")).toHaveTextContent("1");

    // Group filter → recent.
    await user.click(screen.getByText("group-recent-1"));
    expect(screen.getByTestId("active-group")).toHaveTextContent("recent");

    // Back to all, then search.
    await user.click(screen.getByText("group-all-2"));
    await user.type(screen.getByLabelText("search"), "beta");
    expect(screen.getByTestId("count")).toHaveTextContent("1");

    // Open the visible device.
    const table = within(screen.getByTestId("device-table"));
    await user.click(table.getByText("open-d2"));
  });
});
