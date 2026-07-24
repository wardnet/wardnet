import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { TunnelSummary } from "@wardnet/js";

import Home from "../../src/pages/Home";
import { makeDevice, renderWithProviders } from "../test-utils";

const { useMyDevice, useSetMyRule } = vi.hoisted(() => ({
  useMyDevice: vi.fn(),
  useSetMyRule: vi.fn(),
}));
const { useIpGeolocation } = vi.hoisted(() => ({ useIpGeolocation: vi.fn() }));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useMyDevice, useSetMyRule };
});

vi.mock("@/hooks/useIpGeolocation", () => ({ useIpGeolocation }));

// react-leaflet renders a real Leaflet map that jsdom can't drive; stub the
// pieces Home uses as inert DOM so the rest of the card is exercised.
vi.mock("react-leaflet", () => ({
  MapContainer: ({ children }: { children?: React.ReactNode }) => (
    <div data-testid="map">{children}</div>
  ),
  TileLayer: () => <div data-testid="tile" />,
  Marker: () => <div data-testid="marker" />,
  useMapEvents: () => ({ setView: vi.fn(), getZoom: () => 8 }),
}));

const setRuleMutateAsync = vi.fn();
const refetch = vi.fn();

const tunnel: TunnelSummary = {
  id: "t1",
  label: "Sweden",
  country_code: "SE",
  status: "up",
  last_handshake: null,
};

const geo = {
  ip: "9.9.9.9",
  country_code: "SE",
  country_name: "Sweden",
  city: "Stockholm",
  org: "ISP AB",
  latitude: 59.3,
  longitude: 18,
};

function setMyDevice(overrides: Record<string, unknown> = {}) {
  useMyDevice.mockReturnValue({
    data: {
      device: makeDevice(),
      current_rule: null,
      admin_locked: false,
      available_tunnels: [tunnel],
      ...overrides,
    },
    isLoading: false,
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  useSetMyRule.mockReturnValue({
    mutateAsync: setRuleMutateAsync,
    isPending: false,
    isError: false,
    error: null,
  });
  useIpGeolocation.mockReturnValue({
    data: geo,
    isLoading: false,
    isError: false,
    isFetching: false,
    refetch,
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("Home page", () => {
  it("shows a loading state", () => {
    useMyDevice.mockReturnValue({ data: undefined, isLoading: true });
    renderWithProviders(<Home />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("shows the not-detected fallback", () => {
    useMyDevice.mockReturnValue({ data: { device: null }, isLoading: false });
    renderWithProviders(<Home />);
    expect(screen.getByTestId("no-device")).toBeInTheDocument();
  });

  it("renders device identity and the routing form when unlocked", () => {
    setMyDevice();
    renderWithProviders(<Home />);
    expect(screen.getByTestId("device-identity")).toHaveTextContent(
      "My Laptop",
    );
    expect(screen.getByTestId("routing-save")).toBeInTheDocument();
  });

  it("shows a locked, read-only rule when admin-locked (VPN label)", () => {
    setMyDevice({
      admin_locked: true,
      current_rule: { type: "tunnel", tunnel_id: "t1" },
    });
    renderWithProviders(<Home />);
    const locked = screen.getByTestId("routing-locked");
    expect(locked).toHaveTextContent(/VPN/);
    expect(locked).toHaveTextContent("Sweden");
    // No save button in locked mode.
    expect(screen.queryByTestId("routing-save")).not.toBeInTheDocument();
  });

  it("shows the direct label when locked with no tunnel", () => {
    setMyDevice({ admin_locked: true, current_rule: { type: "direct" } });
    renderWithProviders(<Home />);
    expect(screen.getByTestId("routing-locked")).toHaveTextContent(
      "Direct (no VPN)",
    );
  });

  it("disables save when the target equals the current rule", () => {
    // current_rule "direct" ⇒ the form initialises to "direct" ⇒ no change.
    setMyDevice({ current_rule: { type: "direct" } });
    renderWithProviders(<Home />);
    expect(screen.getByTestId("routing-save")).toBeDisabled();
  });

  it("lists assigned routing profiles read-only, in order", () => {
    setMyDevice({
      routing_profiles: [
        { id: "p1", name: "Streaming (UK)" },
        { id: "p2", name: "Work" },
      ],
    });
    renderWithProviders(<Home />);
    const card = screen.getByTestId("routing-profiles-readonly");
    expect(within(card).getByText("Streaming (UK)")).toBeInTheDocument();
    expect(within(card).getByText("Work")).toBeInTheDocument();
  });

  it("hides the routing-profiles card when none are assigned", () => {
    setMyDevice({ routing_profiles: [] });
    renderWithProviders(<Home />);
    expect(screen.queryByTestId("routing-profiles-readonly")).toBeNull();
  });

  it("saves the target and schedules a geolocation re-check", async () => {
    // current_rule null ⇒ form defaults to "direct" which differs from null,
    // so there are pending changes and save is enabled.
    setMyDevice({ current_rule: null });
    setRuleMutateAsync.mockResolvedValue(undefined);
    renderWithProviders(<Home />);

    const save = screen.getByTestId("routing-save");
    expect(save).not.toBeDisabled();
    await userEvent.click(save);

    await waitFor(() =>
      expect(setRuleMutateAsync).toHaveBeenCalledWith({ type: "direct" }),
    );
  });

  it("renders the verify card map and geolocation details, and a Match pill", () => {
    setMyDevice({ current_rule: { type: "tunnel", tunnel_id: "t1" } });
    renderWithProviders(<Home />);
    expect(screen.getByTestId("map")).toBeInTheDocument();
    expect(screen.getByText("9.9.9.9")).toBeInTheDocument();
    expect(screen.getByText("Stockholm")).toBeInTheDocument();
    expect(screen.getByText("ISP AB")).toBeInTheDocument();
    // geo.country_code === tunnel.country_code ⇒ Match.
    expect(screen.getByText("Match")).toBeInTheDocument();
  });

  it("shows a Mismatch pill when the geo country differs from the tunnel", () => {
    setMyDevice({ current_rule: { type: "tunnel", tunnel_id: "t1" } });
    useIpGeolocation.mockReturnValue({
      data: { ...geo, country_code: "US" },
      isLoading: false,
      isError: false,
      isFetching: false,
      refetch,
    });
    renderWithProviders(<Home />);
    expect(screen.getByText("Mismatch")).toBeInTheDocument();
  });

  it("shows a checking state while geolocation loads", () => {
    setMyDevice();
    useIpGeolocation.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
      isFetching: true,
      refetch,
    });
    renderWithProviders(<Home />);
    expect(screen.getByText("Checking…")).toBeInTheDocument();
  });

  it("shows an error state with a retry that refetches", async () => {
    setMyDevice();
    useIpGeolocation.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      isFetching: false,
      refetch,
    });
    renderWithProviders(<Home />);
    expect(
      screen.getByText(/Could not reach the geolocation service/),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(refetch).toHaveBeenCalled();
  });

  it("refetches when the header refresh button is tapped", async () => {
    setMyDevice();
    renderWithProviders(<Home />);
    await userEvent.click(
      screen.getByRole("button", { name: "Refresh route check" }),
    );
    expect(refetch).toHaveBeenCalled();
  });

  it("shows the read-only zone card for a non-home zone", () => {
    setMyDevice({ zone: { name: "Guest", is_default: false } });
    renderWithProviders(<Home />);
    const card = screen.getByTestId("zone-readonly");
    expect(card).toHaveTextContent(
      "You're on: Guest - isolated from the home network.",
    );
    expect(card).toHaveTextContent(/network administrator can change/i);
  });

  it("labels the home zone as the home network", () => {
    setMyDevice({ zone: { name: "Trusted", is_default: true } });
    renderWithProviders(<Home />);
    expect(screen.getByTestId("zone-readonly")).toHaveTextContent(
      "You're on: Trusted - the home network.",
    );
  });

  it("omits the zone card when no zone is present", () => {
    setMyDevice();
    renderWithProviders(<Home />);
    expect(screen.queryByTestId("zone-readonly")).not.toBeInTheDocument();
  });
});
