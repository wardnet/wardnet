import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceZoneCard } from "@/components/features/DeviceZoneCard";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { Device, NetworkZoneView } from "@wardnet/js";

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

function renderCard(device: Device) {
  return renderWithProviders(
    <DeviceZoneCard
      device={device}
      zones={zones}
      assignZone={{
        mutateAsync,
        reset,
        isPending: false,
        isError: false,
        error: null,
      }}
    />,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mutateAsync.mockResolvedValue(undefined);
});

describe("DeviceZoneCard", () => {
  it("shows the device's current zone with a Home pill and the disclaimer", () => {
    renderCard(makeDevice({ zone_id: "zone-1" }));
    expect(screen.getByTestId("device-zone-value")).toHaveTextContent(
      "Trusted",
    );
    expect(screen.getByText("Home")).toBeInTheDocument();
    expect(screen.getByText(/isolated from each other/i)).toBeInTheDocument();
  });

  it("shows a dash when the device's zone is not in the list", () => {
    renderCard(makeDevice({ zone_id: "gone" }));
    expect(screen.getByTestId("device-zone-value")).toHaveTextContent("-");
    expect(screen.queryByText("Home")).not.toBeInTheDocument();
  });

  it("omits the Home pill for a non-default zone", () => {
    renderCard(makeDevice({ zone_id: "zone-2" }));
    expect(screen.getByTestId("device-zone-value")).toHaveTextContent("Guest");
    expect(screen.queryByText("Home")).not.toBeInTheDocument();
  });

  it("enters edit mode and saves the (unchanged) zone", async () => {
    const user = userEvent.setup();
    renderCard(makeDevice({ id: "dev-1", zone_id: "zone-1" }));
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
