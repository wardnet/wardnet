import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { QuarantineSettingsCard } from "@/components/features/QuarantineSettingsCard";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { Device, NetworkZoneView } from "@wardnet/js";

const onSetNotify = vi.fn();
const onSetDefaultForNew = vi.fn();
const onApprove = vi.fn();

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

function renderCard({
  enabled = false,
  pending = [] as Device[],
  approvingDeviceId = null as string | null,
} = {}) {
  return renderWithProviders(
    <QuarantineSettingsCard
      notifyEnabled={enabled}
      notifyPending={false}
      onSetNotify={onSetNotify}
      onSetDefaultForNew={onSetDefaultForNew}
      pending={pending}
      defaultForNew={zones[1]}
      homeZone={zones[0]}
      zones={zones}
      onApprove={onApprove}
      approvingDeviceId={approvingDeviceId}
    />,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("QuarantineSettingsCard", () => {
  it("toggles the new-device notification setting", async () => {
    const user = userEvent.setup();
    renderCard();
    await user.click(screen.getByTestId("quarantine-toggle"));
    expect(onSetNotify).toHaveBeenCalledWith(true);
  });

  it("changes the default-for-new zone via the picker", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.scrollIntoView ??= () => {};
    renderCard();
    await user.click(screen.getByRole("combobox"));
    await user.click(await screen.findByRole("option", { name: "Trusted" }));
    expect(onSetDefaultForNew).toHaveBeenCalledWith("zone-1");
  });

  it("shows an empty review queue when no device sits in the default-for-new zone", () => {
    renderCard();
    expect(screen.getByText(/Awaiting review \(0\)/)).toBeInTheDocument();
    expect(
      screen.getByText(/No new devices awaiting review/i),
    ).toBeInTheDocument();
  });

  it("lists pending devices and approves one to the home zone", async () => {
    const user = userEvent.setup();
    renderCard({
      pending: [
        makeDevice({
          id: "new-1",
          name: "Unknown phone",
          zone_id: "zone-guest",
        }),
      ],
    });
    expect(screen.getByText(/Awaiting review \(1\)/)).toBeInTheDocument();
    expect(screen.getByText("Unknown phone")).toBeInTheDocument();

    await user.click(screen.getByTestId("quarantine-approve"));
    // Approve target defaults to the home (is_default) zone.
    expect(onApprove).toHaveBeenCalledWith("new-1", "zone-1");
  });

  it("disables the approve button for the row whose approval is mid-flight", () => {
    renderCard({
      pending: [
        makeDevice({ id: "new-1", name: "Phone A", zone_id: "zone-guest" }),
        makeDevice({ id: "new-2", name: "Phone B", zone_id: "zone-guest" }),
      ],
      approvingDeviceId: "new-1",
    });
    const buttons = screen.getAllByTestId("quarantine-approve");
    expect(buttons[0]).toBeDisabled();
    expect(buttons[1]).toBeEnabled();
  });
});
