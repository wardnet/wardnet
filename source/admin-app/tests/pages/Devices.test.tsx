import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  useDevices: vi.fn(),
  useTunnels: vi.fn(),
  useDefaultPolicy: vi.fn(),
  useUpdateDevice: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
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
});
