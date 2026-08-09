import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useDevices, useDeviceFilterSettingsList, useDnsFilterProfiles } =
  vi.hoisted(() => ({
    useDevices: vi.fn(),
    useDeviceFilterSettingsList: vi.fn(),
    useDnsFilterProfiles: vi.fn(),
  }));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDevices,
    useDeviceFilterSettingsList,
    useDnsFilterProfiles,
  };
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
    emptyMessage,
  }: {
    devices: Array<{ id: string }>;
    onDeviceClick: (id: string) => void;
    groups: Array<{ id: string; count: number }>;
    activeGroup: string;
    onGroupChange: (id: string) => void;
    searchValue: string;
    onSearchChange: (value: string) => void;
    emptyMessage?: ReactNode;
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
      {/* The real DataTable renders `emptyMessage` in place of rows when there
          are none. The stub must do the same, or the page's empty state — which
          is where the MAC-search help now lives — is never exercised. */}
      {devices.length === 0 && emptyMessage}
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
  useDeviceFilterSettingsList.mockReturnValue({ data: undefined });
  useDnsFilterProfiles.mockReturnValue({ data: undefined });
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

describe("Devices — MAC search dead ends (issue #1099)", () => {
  /** Render the page with a fixed device set and type `query` into search. */
  async function searchFor(macs: string[], query: string) {
    useDevices.mockReturnValue({
      data: {
        devices: macs.map((mac, i) => makeDevice({ id: `d${i}`, mac })),
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<Devices />);
    await userEvent.type(screen.getByLabelText("search"), query);
  }

  it("offers near-MAC candidates when an exact search misses", async () => {
    // The admin types the MAC their vendor app showed. That address is not on
    // the network — but the Wi-Fi address two below it is, which is the whole
    // BLE-vs-Wi-Fi trap the issue reports.
    await searchFor(["5c:e7:53:4e:ec:d9"], "5c:e7:53:4e:ec:db");

    expect(
      screen.getByTestId("neighbour-match-5c:e7:53:4e:ec:d9"),
    ).toBeInTheDocument();
    expect(screen.getByText(/Bluetooth MAC/i)).toBeInTheDocument();
  });

  it("explains the Bluetooth-MAC trap even with no candidates", async () => {
    await searchFor(["aa:bb:cc:dd:ee:ff"], "5c:e7:53:4e:ec:db");

    // No guess to offer, but the admin still must not be left unable to tell
    // "absent" from "I searched the wrong identifier".
    expect(screen.queryByTestId(/^neighbour-match-/)).not.toBeInTheDocument();
    expect(screen.getByText(/Bluetooth MAC/i)).toBeInTheDocument();
  });

  it("stays silent when the search matched something", async () => {
    await searchFor(["5c:e7:53:4e:ec:db"], "5CE7534EECDB");

    expect(screen.queryByText(/Bluetooth MAC/i)).not.toBeInTheDocument();
  });
});

describe("Devices — the dead-end help must not fire on a present device", () => {
  it("stays silent when the match is hidden by the active group", async () => {
    // A device that matches but sits in a tab the admin forgot was selected is
    // present, not missing. Keying the help off the group-filtered list told
    // them "this may be the Bluetooth MAC" and sent them chasing a non-problem.
    useDevices.mockReturnValue({
      data: {
        devices: [
          // Unmanaged: name is null, so the "managed" group excludes it.
          makeDevice({ id: "d0", mac: "5c:e7:53:4e:ec:db", name: null }),
        ],
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<Devices />);

    await userEvent.click(screen.getByText("group-managed-0"));
    await userEvent.type(screen.getByLabelText("search"), "5c:e7:53:4e:ec:db");

    expect(screen.getByTestId("active-group")).toHaveTextContent("managed");
    expect(screen.getByTestId("count")).toHaveTextContent("0");
    expect(screen.queryByText(/Bluetooth MAC/i)).not.toBeInTheDocument();
    // ...and says so plainly, rather than claiming the device does not exist.
    expect(
      screen.getByText(/No devices in this group match/i),
    ).toBeInTheDocument();
  });

  it("shows the plain filter message when nothing is searched", async () => {
    useDevices.mockReturnValue({
      data: { devices: [makeDevice({ id: "d0", name: "Named" })] },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<Devices />);

    // "Unmanaged" excludes the only (named) device, with no search active.
    await userEvent.click(screen.getByText("group-unmanaged-0"));

    expect(
      screen.getByText("No devices match the current filter."),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Bluetooth MAC/i)).not.toBeInTheDocument();
  });
});
