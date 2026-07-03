import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  useDevices: vi.fn(),
  useTunnels: vi.fn(),
  useDefaultPolicy: vi.fn(),
  useUpdateDevice: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
  useNetworkZones: vi.fn(),
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
    h.useTunnels.mockReturnValue({ data: { tunnels: [makeTunnel({ id: "t1", label: "US" })] } });
    h.useDefaultPolicy.mockReturnValue({ data: { policy: "direct" }, isLoading: false });
    h.useUpdateDevice.mockReturnValue({ mutate: vi.fn(), isPending: false });
    // Default: no zones → the "new devices awaiting review" section stays hidden
    // so the existing specs are unaffected.
    h.useNetworkZones.mockReturnValue({ data: { zones: [] } });
    h.useAssignDeviceZone.mockReturnValue({ mutate: h.approve, isPending: false });
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
          makeDevice({ id: "a", name: "Online-A", last_seen: now, current_rule: { type: "tunnel", tunnel_id: "t1" } }),
          makeDevice({ id: "b", name: "Offline-B", last_seen: old }),
        ],
      },
    });
    renderWithProviders(<Devices />);
    expect(screen.getAllByTestId("device-row")).toHaveLength(2);
    expect(screen.getByTestId("device-filter-all")).toHaveTextContent("All (2)");
    expect(screen.getByTestId("device-filter-online")).toHaveTextContent("Online (1)");
    expect(screen.getByTestId("device-filter-vpn")).toHaveTextContent("On VPN (1)");
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
      data: { devices: [makeDevice({ id: "b", name: "Offline-B", last_seen: old })] },
    });
    renderWithProviders(<Devices />);
    await userEvent.click(screen.getByTestId("device-filter-online"));
    expect(screen.getByText("No devices match this filter.")).toBeInTheDocument();
  });

  it("opens the routing sheet when a device row is tapped", async () => {
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: { devices: [makeDevice({ id: "a", name: "Tap-Me", last_seen: now })] },
    });
    renderWithProviders(<Devices />);
    await userEvent.click(screen.getByText("Tap-Me"));
    const sheet = await screen.findByTestId("device-routing-sheet");
    expect(within(sheet).getByTestId("device-routing-default")).toBeInTheDocument();
  });

  it("lists new devices awaiting review and approves one to the home zone", async () => {
    h.useNetworkZones.mockReturnValue({
      data: {
        zones: [
          { id: "z-home", name: "Trusted", is_default: true, is_default_for_new: false },
          { id: "z-guest", name: "Guest", is_default: false, is_default_for_new: true },
        ],
      },
    });
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: {
        devices: [
          makeDevice({ id: "new-1", name: "Unknown", zone_id: "z-guest", first_seen: now, last_seen: now }),
          makeDevice({ id: "known", name: "Laptop", zone_id: "z-home", last_seen: now }),
        ],
      },
    });
    renderWithProviders(<Devices />);

    const section = screen.getByTestId("new-devices-section");
    expect(within(section).getByText("Unknown")).toBeInTheDocument();
    expect(within(section).queryByText("Laptop")).not.toBeInTheDocument();

    await userEvent.click(within(section).getByTestId("new-device-approve"));
    expect(h.approve).toHaveBeenCalledWith({ deviceId: "new-1", zoneId: "z-home" });
  });

  it("lists a pending device without an approve button when there is no home zone", () => {
    h.useNetworkZones.mockReturnValue({
      data: {
        zones: [{ id: "z-guest", name: "Guest", is_default: false, is_default_for_new: true }],
      },
    });
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: { devices: [makeDevice({ id: "new-1", name: "Unknown", zone_id: "z-guest", last_seen: now })] },
    });
    renderWithProviders(<Devices />);
    const section = screen.getByTestId("new-devices-section");
    expect(within(section).getByText("Unknown")).toBeInTheDocument();
    expect(within(section).queryByTestId("new-device-approve")).not.toBeInTheDocument();
  });

  it("hides the review section when no device is in the default-for-new zone", () => {
    h.useNetworkZones.mockReturnValue({
      data: {
        zones: [{ id: "z-guest", name: "Guest", is_default: false, is_default_for_new: true }],
      },
    });
    h.useDevices.mockReturnValue({
      isLoading: false,
      data: { devices: [makeDevice({ id: "a", name: "Laptop", zone_id: "z-home", last_seen: now })] },
    });
    renderWithProviders(<Devices />);
    expect(screen.queryByTestId("new-devices-section")).not.toBeInTheDocument();
  });
});
