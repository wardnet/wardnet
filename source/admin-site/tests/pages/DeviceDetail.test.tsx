import { screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useDevice } = vi.hoisted(() => ({ useDevice: vi.fn() }));

vi.mock("react-router", async (io) => {
  const actual = await io<typeof import("react-router")>();
  return { ...actual, useParams: () => ({ id: "dev-1" }) };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDevice,
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
vi.mock("@/components/features/DeviceDnsFilterCard", () => ({
  DeviceDnsFilterCard: () => <div data-testid="dns-filter-card" />,
}));
vi.mock("@/components/features/DeviceDnsCaptureCard", () => ({
  DeviceDnsCaptureCard: () => <div data-testid="dns-capture-card" />,
}));
vi.mock("@/components/features/DeviceIdentityCard", () => ({
  DeviceIdentityCard: () => <div data-testid="identity-card" />,
}));
vi.mock("@/components/features/DeviceNetworkCard", () => ({
  DeviceNetworkCard: () => <div data-testid="network-card" />,
}));
vi.mock("@/components/features/DeviceSettingsCard", () => ({
  DeviceSettingsCard: () => <div data-testid="settings-card" />,
}));

import DeviceDetail from "@/pages/DeviceDetail";
import { makeDevice, renderWithProviders } from "../test-utils";

beforeEach(() => {
  vi.clearAllMocks();
});

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
    useDevice.mockReturnValue({
      data: {
        device: makeDevice({
          name: "My Laptop",
          last_seen: new Date().toISOString(),
        }),
        current_rule: null,
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("item-label")).toHaveTextContent("My Laptop");
    expect(screen.getByTestId("status-badge")).toHaveTextContent("Online");
    expect(screen.getByTestId("identity-card")).toBeInTheDocument();
  });

  it("renders an offline managed device", () => {
    useDevice.mockReturnValue({
      data: {
        device: makeDevice({
          name: "Old Device",
          last_seen: "2000-01-01T00:00:00Z",
        }),
        current_rule: null,
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("status-badge")).toHaveTextContent("Offline");
  });

  it("renders a discovered (unmanaged) device and falls back to hostname label", () => {
    useDevice.mockReturnValue({
      data: {
        device: makeDevice({ name: null, hostname: "living-room-tv" }),
        current_rule: null,
      },
      isLoading: false,
      isError: false,
    });
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
    useDevice.mockReturnValue({
      data: {
        device: makeDevice({
          name: null,
          hostname: null,
          manufacturer: "Govee",
          manufacturer_source: "catalog",
          mac: "5c:e7:53:4e:ec:d9",
        }),
        current_rule: null,
      },
      isLoading: false,
      isError: false,
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
    useDevice.mockReturnValue({
      data: {
        device: makeDevice({
          name: null,
          hostname: null,
          manufacturer: "Acme",
          manufacturer_source: "ieee",
        }),
        current_rule: null,
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("item-label")).toHaveTextContent("Acme device");
  });

  it("falls back to MAC label", () => {
    useDevice.mockReturnValue({
      data: {
        device: makeDevice({
          name: null,
          hostname: null,
          manufacturer: null,
          mac: "AA:BB:CC:00:11:22",
        }),
        current_rule: null,
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("item-label")).toHaveTextContent(
      "AA:BB:CC:00:11:22",
    );
  });

  it("treats an invalid last_seen as offline for managed devices", () => {
    useDevice.mockReturnValue({
      data: {
        device: makeDevice({ name: "Weird", last_seen: "not-a-date" }),
        current_rule: null,
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<DeviceDetail />);
    expect(screen.getByTestId("status-badge")).toHaveTextContent("Offline");
  });
});
