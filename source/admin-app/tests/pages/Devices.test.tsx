import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  useDevices: vi.fn(),
  useTunnels: vi.fn(),
  useDefaultPolicy: vi.fn(),
  useUpdateDevice: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
  // Typed return so the empty default isn't inferred as `never[]`, which would
  // reject the populated-zone override in the zone-reassignment spec.
  useNetworkZones: vi.fn(
    (): {
      data: { zones: Array<{ id: string; name: string; is_default: boolean }> };
    } => ({ data: { zones: [] } }),
  ),
  usePendingDevices: vi.fn(),
  useAssignDeviceZone: vi.fn(),
  approve: vi.fn(),
  ctx: { value: { showingLastKnownState: false } },
}));
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDevices: h.useDevices,
    useTunnels: h.useTunnels,
    useDefaultPolicy: h.useDefaultPolicy,
    useUpdateDevice: h.useUpdateDevice,
    useNetworkZones: h.useNetworkZones,
    usePendingDevices: h.usePendingDevices,
    useAssignDeviceZone: h.useAssignDeviceZone,
  };
});
vi.mock("@/context/OnlineStatusContext", () => ({
  useOnlineStatusContext: () => h.ctx.value,
}));

import Devices from "@/pages/Devices";
import { renderWithProviders, makeDevice, makeTunnel } from "../test-utils";

const now = new Date().toISOString();
const old = "2000-01-01T00:00:00Z";

describe("Devices page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.ctx.value = { showingLastKnownState: false };
    h.useTunnels.mockReturnValue({
      data: { tunnels: [makeTunnel({ id: "t1", label: "US" })] },
    });
    h.useDefaultPolicy.mockReturnValue({
      data: { policy: "direct" },
      isLoading: false,
    });
    h.useUpdateDevice.mockReturnValue({ mutate: vi.fn(), isPending: false });
    h.useNetworkZones.mockReturnValue({ data: { zones: [] } });
    // Default: no pending devices → the "awaiting review" section stays hidden
    // so the existing specs are unaffected.
    h.usePendingDevices.mockReturnValue({
      pending: [],
      homeZone: undefined,
      defaultForNew: undefined,
      zones: [],
    });
    h.useAssignDeviceZone.mockReturnValue({
      mutate: h.approve,
      isPending: false,
    });
  });

  it("shows a skeleton while loading", () => {
    h.useDevices.mockReturnValue({ data: undefined, isLoading: true });
    h.useDefaultPolicy.mockReturnValue({ data: undefined, isLoading: true });
    const { container } = renderWithProviders(<Devices />);
    expect(screen.getByText("Devices")).toBeInTheDocument();
    expect(container.querySelector(".animate-pulse")).not.toBeNull();
  });

  it("lists devices and counts across filter pills", () => {
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: {
        devices: [
          makeDevice({
            id: "a",
            name: "Online-A",
            last_seen: now,
            current_rule: { type: "tunnel", tunnel_id: "t1" },
          }),
          makeDevice({ id: "b", name: "Offline-B", last_seen: old }),
        ],
      },
    });
    renderWithProviders(<Devices />);
    expect(screen.getAllByTestId("device-row")).toHaveLength(2);
    expect(screen.getByTestId("device-filter-all")).toHaveTextContent(
      "All (2)",
    );
    expect(screen.getByTestId("device-filter-online")).toHaveTextContent(
      "Online (1)",
    );
    expect(screen.getByTestId("device-filter-vpn")).toHaveTextContent(
      "On VPN (1)",
    );
  });

  it("filters to online devices only", async () => {
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: {
        devices: [
          makeDevice({ id: "a", name: "Online-A", last_seen: now }),
          makeDevice({ id: "b", name: "Offline-B", last_seen: old }),
        ],
      },
    });
    renderWithProviders(<Devices />);
    await userEvent.click(screen.getByTestId("device-filter-online"));
    expect(screen.getAllByTestId("device-row")).toHaveLength(1);
    expect(screen.getByText("Online-A")).toBeInTheDocument();
  });

  it("shows the empty message when the filter matches nothing", async () => {
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: {
        devices: [makeDevice({ id: "b", name: "Offline-B", last_seen: old })],
      },
    });
    renderWithProviders(<Devices />);
    await userEvent.click(screen.getByTestId("device-filter-online"));
    expect(
      screen.getByText("No devices match this filter."),
    ).toBeInTheDocument();
  });

  it("opens the routing sheet when a device row is tapped", async () => {
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: {
        devices: [makeDevice({ id: "a", name: "Tap-Me", last_seen: now })],
      },
    });
    renderWithProviders(<Devices />);
    await userEvent.click(screen.getByText("Tap-Me"));
    const sheet = await screen.findByTestId("device-routing-sheet");
    expect(
      within(sheet).getByTestId("device-routing-default"),
    ).toBeInTheDocument();
  });

  it("routes the tapped device and closes the sheet on success", async () => {
    const mutate = vi.fn((_vars, opts) => opts?.onSuccess?.());
    h.useUpdateDevice.mockReturnValue({ mutate, isPending: false });
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: {
        devices: [makeDevice({ id: "a", name: "Tap-Me", last_seen: now })],
      },
    });
    renderWithProviders(<Devices />);

    await userEvent.click(screen.getByText("Tap-Me"));
    const sheet = await screen.findByTestId("device-routing-sheet");
    await userEvent.click(within(sheet).getByTestId("device-routing-default"));

    expect(mutate).toHaveBeenCalledWith(
      { id: "a", body: { routing_target: { type: "default" } } },
      expect.any(Object),
    );
    // The mutation's onSuccess closes the sheet (Radix unmounts the content).
    await waitFor(() =>
      expect(
        screen.queryByTestId("device-routing-sheet"),
      ).not.toBeInTheDocument(),
    );
  });

  it("reassigns the tapped device's zone and closes the sheet on success", async () => {
    const assign = vi.fn((_vars, opts) => opts?.onSuccess?.());
    h.useAssignDeviceZone.mockReturnValue({ mutate: assign, isPending: false });
    h.useNetworkZones.mockReturnValue({
      data: {
        zones: [
          { id: "z1", name: "Trusted", is_default: true },
          { id: "z2", name: "Guest", is_default: false },
        ],
      },
    });
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: {
        devices: [
          makeDevice({
            id: "a",
            name: "Tap-Me",
            zone_id: "z1",
            last_seen: now,
          }),
        ],
      },
    });
    renderWithProviders(<Devices />);

    await userEvent.click(screen.getByText("Tap-Me"));
    const sheet = await screen.findByTestId("device-routing-sheet");
    await userEvent.click(within(sheet).getByText("Guest"));

    expect(assign).toHaveBeenCalledWith(
      { deviceId: "a", zoneId: "z2" },
      expect.any(Object),
    );
    await waitFor(() =>
      expect(
        screen.queryByTestId("device-routing-sheet"),
      ).not.toBeInTheDocument(),
    );
  });

  it("lists new devices awaiting review and approves one to the home zone", async () => {
    h.useDevices.mockReturnValue({ isLoading: false, data: { devices: [] } });
    h.usePendingDevices.mockReturnValue({
      pending: [
        makeDevice({
          id: "new-1",
          name: "Unknown",
          zone_id: "z-guest",
          first_seen: now,
          last_seen: now,
        }),
      ],
      homeZone: { id: "z-home", name: "Trusted", is_default: true },
      defaultForNew: { id: "z-guest" },
      zones: [],
    });
    renderWithProviders(<Devices />);

    const section = screen.getByTestId("new-devices-section");
    expect(within(section).getByText("Unknown")).toBeInTheDocument();

    await userEvent.click(within(section).getByTestId("new-device-approve"));
    expect(h.approve).toHaveBeenCalledWith({
      deviceId: "new-1",
      zoneId: "z-home",
    });
  });

  it("lists a pending device without an approve button when there is no home zone", () => {
    h.useDevices.mockReturnValue({ isLoading: false, data: { devices: [] } });
    h.usePendingDevices.mockReturnValue({
      pending: [
        makeDevice({
          id: "new-1",
          name: "Unknown",
          zone_id: "z-guest",
          last_seen: now,
        }),
      ],
      homeZone: undefined,
      defaultForNew: { id: "z-guest" },
      zones: [],
    });
    renderWithProviders(<Devices />);
    const section = screen.getByTestId("new-devices-section");
    expect(within(section).getByText("Unknown")).toBeInTheDocument();
    expect(
      within(section).queryByTestId("new-device-approve"),
    ).not.toBeInTheDocument();
  });

  it("hides the review section when there are no pending devices", () => {
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: {
        devices: [
          makeDevice({
            id: "a",
            name: "Laptop",
            zone_id: "z-home",
            last_seen: now,
          }),
        ],
      },
    });
    h.usePendingDevices.mockReturnValue({
      pending: [],
      homeZone: undefined,
      defaultForNew: undefined,
      zones: [],
    });
    renderWithProviders(<Devices />);
    expect(screen.queryByTestId("new-devices-section")).not.toBeInTheDocument();
  });
});

describe("Devices — MAC search (issue #1099)", () => {
  /** Render the list with a fixed device set and type into the search box. */
  async function searchFor(
    devices: Parameters<typeof makeDevice>[0][],
    q: string,
  ) {
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: { devices: devices.map((d) => makeDevice(d)) },
    });
    renderWithProviders(<Devices />);
    await userEvent.type(screen.getByTestId("device-search"), q);
  }

  it("filters the list by a format-tolerant MAC", async () => {
    await searchFor(
      [
        { id: "a", name: "Lamp", mac: "5c:e7:53:4e:ec:d9" },
        { id: "b", name: "Laptop", mac: "a8:bb:cc:dd:ee:01" },
      ],
      "5CE7534EECD9",
    );

    expect(screen.getByText("Lamp")).toBeInTheDocument();
    expect(screen.queryByText("Laptop")).not.toBeInTheDocument();
  });

  it("offers near-MAC candidates when an exact search misses", async () => {
    // The admin types the MAC their vendor app printed — the BLE address,
    // two below the Wi-Fi one the device actually associated with.
    await searchFor(
      [{ id: "a", name: "Lamp", mac: "5c:e7:53:4e:ec:d9" }],
      "5c:e7:53:4e:ec:db",
    );

    expect(
      screen.getByTestId("neighbour-match-5c:e7:53:4e:ec:d9"),
    ).toBeInTheDocument();
    expect(screen.getByText(/Bluetooth MAC/i)).toBeInTheDocument();
  });

  it("explains the Bluetooth-MAC trap even with no candidates", async () => {
    await searchFor(
      [{ id: "a", name: "Lamp", mac: "a8:bb:cc:dd:ee:01" }],
      "5c:e7:53:4e:ec:db",
    );

    expect(screen.queryByTestId(/^neighbour-match-/)).not.toBeInTheDocument();
    expect(screen.getByText(/Bluetooth MAC/i)).toBeInTheDocument();
  });

  it("does not claim a device is missing when a filter merely hides it", async () => {
    // The device matches but the "On VPN" pill excludes it — saying "no device
    // matches" would send the admin hunting for something that is right there.
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: {
        devices: [
          makeDevice({ id: "a", name: "Lamp", mac: "5c:e7:53:4e:ec:d9" }),
        ],
      },
    });
    renderWithProviders(<Devices />);

    await userEvent.click(screen.getByTestId("device-filter-vpn"));
    await userEvent.type(
      screen.getByTestId("device-search"),
      "5c:e7:53:4e:ec:d9",
    );

    expect(
      screen.getByText(/No devices in this filter match/i),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Bluetooth MAC/i)).not.toBeInTheDocument();
  });

  it("badges a randomized MAC on the row rather than as a manufacturer", async () => {
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: {
        devices: [makeDevice({ id: "a", name: "Phone", is_randomized: true })],
      },
    });
    renderWithProviders(<Devices />);

    expect(screen.getByText(/Randomized MAC/)).toBeInTheDocument();
  });
});
