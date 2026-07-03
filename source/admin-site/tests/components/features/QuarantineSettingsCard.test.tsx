import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { QuarantineSettingsCard } from "@/components/features/QuarantineSettingsCard";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { NetworkZoneView } from "@wardnet/js";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useQuarantineNewDevices: vi.fn(),
    useSetQuarantineNewDevices: vi.fn(),
    useNetworkZones: vi.fn(),
    useUpdateNetworkZone: vi.fn(),
    useDevices: vi.fn(),
    useAssignDeviceZone: vi.fn(),
  };
});

import {
  useQuarantineNewDevices,
  useSetQuarantineNewDevices,
  useNetworkZones,
  useUpdateNetworkZone,
  useDevices,
  useAssignDeviceZone,
} from "@wardnet/web";

const setQuarantine = vi.fn();
const setDefaultForNew = vi.fn();
const assign = vi.fn();

function makeZone(over: Partial<NetworkZoneView> = {}): NetworkZoneView {
  return {
    id: "zone-1",
    name: "Trusted",
    provenance: "system",
    isolation_stance: "shared_subnet",
    allowed_targets: ["direct", "tunnel"],
    member_isolation: false,
    subnet: null,
    admin_ui_reachable: true,
    is_default: true,
    is_default_for_new: false,
    member_count: 0,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...over,
  };
}

const zones = [
  makeZone(),
  makeZone({
    id: "zone-guest",
    name: "Guest",
    is_default: false,
    is_default_for_new: true,
  }),
];

function setup({
  enabled = false,
  devices = [] as ReturnType<typeof makeDevice>[],
} = {}) {
  vi.mocked(useQuarantineNewDevices).mockReturnValue({
    data: { enabled },
  } as unknown as ReturnType<typeof useQuarantineNewDevices>);
  vi.mocked(useSetQuarantineNewDevices).mockReturnValue({
    mutate: setQuarantine,
    isPending: false,
  } as unknown as ReturnType<typeof useSetQuarantineNewDevices>);
  vi.mocked(useNetworkZones).mockReturnValue({
    data: { zones },
  } as unknown as ReturnType<typeof useNetworkZones>);
  vi.mocked(useUpdateNetworkZone).mockReturnValue({
    mutate: setDefaultForNew,
  } as unknown as ReturnType<typeof useUpdateNetworkZone>);
  vi.mocked(useDevices).mockReturnValue({
    data: { devices },
  } as unknown as ReturnType<typeof useDevices>);
  vi.mocked(useAssignDeviceZone).mockReturnValue({
    mutate: assign,
    isPending: false,
  } as unknown as ReturnType<typeof useAssignDeviceZone>);
}

beforeEach(() => {
  vi.clearAllMocks();
  setup();
});

describe("QuarantineSettingsCard", () => {
  it("toggles the new-device notification setting", async () => {
    const user = userEvent.setup();
    renderWithProviders(<QuarantineSettingsCard />);
    await user.click(screen.getByTestId("quarantine-toggle"));
    expect(setQuarantine).toHaveBeenCalledWith(true);
  });

  it("shows an empty review queue when no device sits in the default-for-new zone", () => {
    renderWithProviders(<QuarantineSettingsCard />);
    expect(screen.getByText(/Awaiting review \(0\)/)).toBeInTheDocument();
    expect(
      screen.getByText(/No new devices awaiting review/i),
    ).toBeInTheDocument();
  });

  it("lists pending devices and approves one to the home zone", async () => {
    const user = userEvent.setup();
    setup({
      devices: [
        makeDevice({
          id: "new-1",
          name: "Unknown phone",
          zone_id: "zone-guest",
        }),
        makeDevice({ id: "old-1", name: "Laptop", zone_id: "zone-1" }),
      ],
    });
    renderWithProviders(<QuarantineSettingsCard />);
    expect(screen.getByText(/Awaiting review \(1\)/)).toBeInTheDocument();
    expect(screen.getByText("Unknown phone")).toBeInTheDocument();
    expect(screen.queryByText("Laptop")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("quarantine-approve"));
    await waitFor(() => expect(assign).toHaveBeenCalled());
    // Approve target defaults to the home (is_default) zone.
    expect(assign).toHaveBeenCalledWith({
      deviceId: "new-1",
      zoneId: "zone-1",
    });
  });
});
