import { screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useDevice,
  useTunnels,
  useUpdateDevice,
  useIdentifyDevice,
  useRoutingProfiles,
  useDeviceRoutingProfiles,
  useSetDeviceRoutingProfiles,
  useNetworkZones,
  useAssignDeviceZone,
  useDeviceFilterSettings,
  useDnsFilterProfiles,
  useDnsFilterConfig,
  useUpdateDeviceFilterSettings,
  useDnsCaptureSettings,
  useUpdateDnsCaptureSettings,
  useDhcpReservations,
  useCreateReservation,
  useDeleteReservation,
} = vi.hoisted(() => ({
  useDevice: vi.fn(),
  useTunnels: vi.fn(),
  useUpdateDevice: vi.fn(),
  useIdentifyDevice: vi.fn(),
  useRoutingProfiles: vi.fn(),
  useDeviceRoutingProfiles: vi.fn(),
  useSetDeviceRoutingProfiles: vi.fn(),
  useNetworkZones: vi.fn(),
  useAssignDeviceZone: vi.fn(),
  useDeviceFilterSettings: vi.fn(),
  useDnsFilterProfiles: vi.fn(),
  useDnsFilterConfig: vi.fn(),
  useUpdateDeviceFilterSettings: vi.fn(),
  useDnsCaptureSettings: vi.fn(),
  useUpdateDnsCaptureSettings: vi.fn(),
  useDhcpReservations: vi.fn(),
  useCreateReservation: vi.fn(),
  useDeleteReservation: vi.fn(),
}));

vi.mock("react-router", async (io) => {
  const actual = await io<typeof import("react-router")>();
  return { ...actual, useParams: () => ({ id: "dev-1" }) };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDevice,
    useTunnels,
    useUpdateDevice,
    useIdentifyDevice,
    useRoutingProfiles,
    useDeviceRoutingProfiles,
    useSetDeviceRoutingProfiles,
    useNetworkZones,
    useAssignDeviceZone,
    useDeviceFilterSettings,
    useDnsFilterProfiles,
    useDnsFilterConfig,
    useUpdateDeviceFilterSettings,
    useDnsCaptureSettings,
    useUpdateDnsCaptureSettings,
    useDhcpReservations,
    useCreateReservation,
    useDeleteReservation,
    DeviceIcon: ({ type }: { type: string }) => (
      <span data-testid="device-icon">{type}</span>
    ),
  };
});

vi.mock("@/components/compound/DetailPageHeader", () => ({
  DetailPageHeader: ({
    itemLabel,
    status,
    meta,
  }: {
    itemLabel: ReactNode;
    status: ReactNode;
    meta: ReactNode;
  }) => (
    <div data-testid="detail-header">
      <span data-testid="item-label">{itemLabel}</span>
      <span data-testid="status">{status}</span>
      <span data-testid="meta">{meta}</span>
    </div>
  ),
}));
vi.mock("@/components/compound/StatusBadge", () => ({
  StatusBadge: ({ children, tone }: { children: ReactNode; tone?: string }) => (
    <span data-testid="status-badge" data-tone={tone}>
      {children}
    </span>
  ),
}));
// The cards are mocked to thin harnesses that surface the page's wiring so
// the tests assert what the page hoisted, not the cards' own rendering.
vi.mock("@/components/features/DeviceDnsFilterCard", () => ({
  DeviceDnsFilterCard: ({
    settings,
    profiles,
    config,
  }: {
    settings?: { enabled: boolean };
    profiles?: unknown[];
    config?: unknown;
  }) => (
    <div
      data-testid="dns-filter-card"
      data-enabled={String(settings?.enabled)}
      data-profiles={profiles?.length ?? "none"}
      data-config={String(config != null)}
    />
  ),
}));
vi.mock("@/components/features/DeviceDnsCaptureCard", () => ({
  DeviceDnsCaptureCard: ({
    settings,
  }: {
    settings?: { cap_count: number };
  }) => (
    <div
      data-testid="dns-capture-card"
      data-cap={settings?.cap_count ?? "none"}
    />
  ),
}));
vi.mock("@/components/features/DeviceIdentityCard", () => ({
  DeviceIdentityCard: () => <div data-testid="identity-card" />,
}));
vi.mock("@/components/features/DeviceIdentificationCard", () => ({
  DeviceIdentificationCard: ({
    signals,
    online,
    deviceId,
  }: {
    signals: unknown[];
    online: boolean;
    deviceId: string;
  }) => (
    <div
      data-testid="identification-card"
      data-count={signals.length}
      data-online={String(online)}
      data-device-id={deviceId}
    />
  ),
}));
vi.mock("@/components/features/DeviceNetworkCard", () => ({
  DeviceNetworkCard: ({
    reservations,
  }: {
    reservations?: Array<{ id: string }>;
  }) => (
    <div
      data-testid="network-card"
      data-reservations={reservations?.length ?? "none"}
    />
  ),
}));
vi.mock("@/components/features/DeviceSettingsCard", () => ({
  DeviceSettingsCard: ({ tunnels }: { tunnels: Array<{ id: string }> }) => (
    <div data-testid="settings-card" data-tunnels={tunnels.length} />
  ),
}));
vi.mock("@/components/features/DeviceZoneCard", () => ({
  DeviceZoneCard: ({ zones }: { zones: Array<{ id: string }> }) => (
    <div data-testid="zone-card" data-zones={zones.length} />
  ),
}));
vi.mock("@/components/features/DeviceRoutingProfilesCard", () => ({
  DeviceRoutingProfilesCard: ({
    allProfiles,
    assignedIds,
  }: {
    allProfiles: Array<{ id: string }>;
    assignedIds: string[];
  }) => (
    <div
      data-testid="routing-profiles-card"
      data-profiles={allProfiles.length}
      data-assigned={assignedIds.join(",")}
    />
  ),
}));

import DeviceDetail from "@/pages/DeviceDetail";
import { makeDevice, renderWithProviders } from "../test-utils";

function mutationHandle() {
  return {
    mutateAsync: vi.fn(),
    reset: vi.fn(),
    isPending: false,
    isError: false,
    error: null,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useTunnels.mockReturnValue({ data: { tunnels: [{ id: "tun-1" }] } });
  useUpdateDevice.mockReturnValue(mutationHandle());
  useIdentifyDevice.mockReturnValue(mutationHandle());
  useRoutingProfiles.mockReturnValue({ data: { profiles: [{ id: "p1" }] } });
  useDeviceRoutingProfiles.mockReturnValue({
    data: { profile_ids: ["p1"] },
  });
  useSetDeviceRoutingProfiles.mockReturnValue(mutationHandle());
  useNetworkZones.mockReturnValue({ data: { zones: [{ id: "z1" }] } });
  useAssignDeviceZone.mockReturnValue(mutationHandle());
  useDeviceFilterSettings.mockReturnValue({
    data: { settings: { enabled: true, profile_ids: [] } },
    isLoading: false,
  });
  useDnsFilterProfiles.mockReturnValue({
    data: { profiles: [{ id: "fp1" }] },
    isLoading: false,
  });
  useDnsFilterConfig.mockReturnValue({
    data: { config: { enabled: true, default_profile_ids: [] } },
    isLoading: false,
  });
  useUpdateDeviceFilterSettings.mockReturnValue(mutationHandle());
  useDnsCaptureSettings.mockReturnValue({
    data: { enabled: true, cap_count: 1000, cap_days: 7 },
    isLoading: false,
  });
  useUpdateDnsCaptureSettings.mockReturnValue(mutationHandle());
  useDhcpReservations.mockReturnValue({
    data: { reservations: [{ id: "res-1" }] },
  });
  useCreateReservation.mockReturnValue(mutationHandle());
  useDeleteReservation.mockReturnValue(mutationHandle());
});

function loadDevice(overrides = {}) {
  useDevice.mockReturnValue({
    data: {
      device: makeDevice(overrides),
      current_rule: null,
      signals: [],
    },
    isLoading: false,
    isError: false,
  });
}

describe("DeviceDetail", () => {
  it("shows loading state", () => {
    useDevice.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("shows not-found on error", () => {
    useDevice.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByText("Device not found")).toBeInTheDocument();
    expect(screen.getByText("Back to Devices")).toBeInTheDocument();
  });

  it("shows not-found when data missing without error", () => {
    useDevice.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByText("Device not found")).toBeInTheDocument();
  });

  it("renders an online managed device", () => {
    loadDevice({
      name: "My Laptop",
      managed: true,
      last_seen: new Date().toISOString(),
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("item-label")).toHaveTextContent("My Laptop");
    expect(screen.getByTestId("status-badge")).toHaveTextContent("Online");
    expect(screen.getByTestId("identity-card")).toBeInTheDocument();
  });

  it("hands each card the data its hoisted queries resolved", () => {
    loadDevice({ name: "My Laptop" });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("settings-card")).toHaveAttribute(
      "data-tunnels",
      "1",
    );
    expect(screen.getByTestId("routing-profiles-card")).toHaveAttribute(
      "data-profiles",
      "1",
    );
    expect(screen.getByTestId("routing-profiles-card")).toHaveAttribute(
      "data-assigned",
      "p1",
    );
    expect(screen.getByTestId("zone-card")).toHaveAttribute("data-zones", "1");
    expect(screen.getByTestId("dns-filter-card")).toHaveAttribute(
      "data-enabled",
      "true",
    );
    expect(screen.getByTestId("dns-filter-card")).toHaveAttribute(
      "data-profiles",
      "1",
    );
    expect(screen.getByTestId("dns-capture-card")).toHaveAttribute(
      "data-cap",
      "1000",
    );
    expect(screen.getByTestId("network-card")).toHaveAttribute(
      "data-reservations",
      "1",
    );
  });

  it("passes no config to the filter card until its query settles", () => {
    loadDevice({ name: "My Laptop" });
    useDnsFilterConfig.mockReturnValue({ data: undefined, isLoading: true });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("dns-filter-card")).toHaveAttribute(
      "data-config",
      "false",
    );
  });

  it("passes the device's identification signals to the identification card", () => {
    // The signals are the provenance behind the manufacturer shown on the
    // identity card, so the detail page has to hand them down (issue #1099).
    useDevice.mockReturnValue({
      data: {
        device: makeDevice({ name: "Lamp" }),
        current_rule: null,
        signals: [
          {
            kind: "mdns_service",
            value: "_govee._tcp",
            inferred: true,
            observed_at: "2026-08-05T10:00:00Z",
          },
        ],
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("identification-card")).toHaveAttribute(
      "data-count",
      "1",
    );
  });

  it("lets an unmanaged but present device be identified", () => {
    // An unnamed device is precisely the one worth probing, so the card's
    // `online` prop must be raw presence rather than the header's
    // managed-and-online status.
    loadDevice({ name: null, last_seen: new Date().toISOString() });
    renderWithProviders(<DeviceDetail />);
    const card = screen.getByTestId("identification-card");
    expect(card).toHaveAttribute("data-online", "true");
    expect(card).toHaveAttribute("data-device-id", "dev-1");
  });

  it("marks a departed device as unprobeable", () => {
    // The daemon refuses the probe anyway; disabling it up front is what keeps
    // the button from looking broken.
    loadDevice({ name: "Old Device", last_seen: "2000-01-01T00:00:00Z" });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("identification-card")).toHaveAttribute(
      "data-online",
      "false",
    );
  });

  it("keys the identification card by device so its probe note cannot follow the admin", () => {
    // The card holds the transient "nothing answered" result (it is not
    // persisted by design). Without a key, navigating between two already-
    // cached devices reconciles the same element and the note would make a
    // claim about a device that was never probed.
    loadDevice({ id: "dev-1", last_seen: new Date().toISOString() });
    const { rerender } = renderWithProviders(<DeviceDetail />);
    const first = screen.getByTestId("identification-card");

    loadDevice({ id: "dev-2", last_seen: new Date().toISOString() });
    rerender(<DeviceDetail />);
    const second = screen.getByTestId("identification-card");

    expect(second).toHaveAttribute("data-device-id", "dev-2");
    expect(second).not.toBe(first);
  });

  it("renders an offline managed device", () => {
    loadDevice({
      name: "Old Device",
      managed: true,
      last_seen: "2000-01-01T00:00:00Z",
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("status-badge")).toHaveTextContent("Offline");
  });

  it("renders a discovered (unmanaged) device and falls back to hostname label", () => {
    loadDevice({ name: null, managed: false, hostname: "living-room-tv" });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("status-badge")).toHaveTextContent("Discovered");
    expect(screen.getByTestId("item-label")).toHaveTextContent(
      "living-room-tv",
    );
  });

  it("does not title the page after a hedged manufacturer guess", () => {
    // A curated-catalog match is an inference, not a registered fact. Titling
    // the page "Govee device" would state it as fact — exactly the hedge the
    // rest of the UI applies via manufacturerDisplay (issue #1099).
    loadDevice({
      name: null,
      hostname: null,
      manufacturer: "Govee",
      manufacturer_source: "catalog",
      mac: "5c:e7:53:4e:ec:d9",
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("item-label")).toHaveTextContent(
      "5c:e7:53:4e:ec:d9",
    );
    expect(screen.getByTestId("item-label")).not.toHaveTextContent(
      "Govee device",
    );
  });

  it("falls back to manufacturer label for an IEEE registrant", () => {
    loadDevice({
      name: null,
      hostname: null,
      manufacturer: "Acme",
      manufacturer_source: "ieee",
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("item-label")).toHaveTextContent("Acme device");
  });

  it("falls back to MAC label", () => {
    loadDevice({
      name: null,
      hostname: null,
      manufacturer: null,
      mac: "AA:BB:CC:00:11:22",
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("item-label")).toHaveTextContent(
      "AA:BB:CC:00:11:22",
    );
  });

  it("treats an invalid last_seen as offline for managed devices", () => {
    loadDevice({ name: "Weird", managed: true, last_seen: "not-a-date" });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("status-badge")).toHaveTextContent("Offline");
  });
});
