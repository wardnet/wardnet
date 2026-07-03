import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceZoneCard } from "@/components/features/DeviceZoneCard";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { NetworkZoneView } from "@wardnet/js";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useNetworkZones: vi.fn(), useAssignDeviceZone: vi.fn() };
});

import { useNetworkZones, useAssignDeviceZone } from "@wardnet/web";

const mutateAsync = vi.fn();
const reset = vi.fn();

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
  makeZone({ id: "zone-2", name: "Guest", is_default: false }),
];

function setup() {
  vi.mocked(useNetworkZones).mockReturnValue({
    data: { zones },
  } as unknown as ReturnType<typeof useNetworkZones>);
  vi.mocked(useAssignDeviceZone).mockReturnValue({
    mutateAsync,
    reset,
    isPending: false,
  } as unknown as ReturnType<typeof useAssignDeviceZone>);
}

beforeEach(() => {
  vi.clearAllMocks();
  mutateAsync.mockResolvedValue(undefined);
  setup();
});

describe("DeviceZoneCard", () => {
  it("shows the device's current zone with a Home pill and the disclaimer", () => {
    renderWithProviders(
      <DeviceZoneCard device={makeDevice({ zone_id: "zone-1" })} />,
    );
    expect(screen.getByTestId("device-zone-value")).toHaveTextContent(
      "Trusted",
    );
    expect(screen.getByText("Home")).toBeInTheDocument();
    expect(screen.getByText(/isolated from each other/i)).toBeInTheDocument();
  });

  it("enters edit mode and saves the (unchanged) zone", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceZoneCard
        device={makeDevice({ id: "dev-1", zone_id: "zone-1" })}
      />,
    );
    await user.click(screen.getByTestId("device-zone-edit"));
    expect(reset).toHaveBeenCalled();
    await user.click(screen.getByTestId("device-zone-save"));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
    expect(mutateAsync).toHaveBeenCalledWith({
      deviceId: "dev-1",
      zoneId: "zone-1",
    });
  });
});
