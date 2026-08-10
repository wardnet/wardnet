import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DeviceProbeResponse, DeviceSignal } from "@wardnet/js";

import { DeviceIdentificationCard } from "@/components/features/DeviceIdentificationCard";
import { renderWithProviders } from "../../test-utils";

const mutateAsync = vi.fn();
const reset = vi.fn();

beforeEach(() => {
  mutateAsync.mockReset();
  reset.mockReset();
});

function signal(overrides: Partial<DeviceSignal> = {}): DeviceSignal {
  return {
    kind: "mdns_service",
    value: "_govee._tcp",
    inferred: false,
    observed_at: "2026-08-05T10:00:00Z",
    ...overrides,
  };
}

function probeResult(
  overrides: Partial<DeviceProbeResponse> = {},
): DeviceProbeResponse {
  return {
    ports_probed: [4003, 6053, 6668, 8009],
    answering_ports: [6668],
    device: {} as DeviceProbeResponse["device"],
    ...overrides,
  };
}

function renderCard(
  props: {
    signals?: DeviceSignal[];
    online?: boolean;
    isPending?: boolean;
  } = {},
) {
  return renderWithProviders(
    <DeviceIdentificationCard
      deviceId="device-1"
      signals={props.signals ?? []}
      online={props.online ?? true}
      identify={{
        mutateAsync,
        reset,
        isPending: props.isPending ?? false,
        isError: false,
        error: null,
      }}
    />,
  );
}

describe("DeviceIdentificationCard", () => {
  it("explains an empty list instead of showing a bare blank", () => {
    // A device seen only by ARP has no signals, and that must not read as a
    // Wardnet failure — the confusion issue #1099 was filed about.
    renderCard();
    expect(screen.getByText(/Nothing observed yet/)).toBeInTheDocument();
    // The copy must not promise observation Wardnet does not yet perform:
    // passive mDNS browsing is #1115, not shipped here.
    expect(screen.queryByText(/announces a service/)).not.toBeInTheDocument();
  });

  it("groups signals under a per-kind heading", () => {
    renderCard({
      signals: [
        signal({ value: "_govee._tcp" }),
        signal({ kind: "dhcp_hostname", value: "govee-lamp" }),
      ],
    });
    expect(screen.getByText("Announced services (mDNS)")).toBeInTheDocument();
    expect(screen.getByText("Hostname (DHCP)")).toBeInTheDocument();
    expect(screen.getByText("_govee._tcp")).toBeInTheDocument();
    expect(screen.getByText("govee-lamp")).toBeInTheDocument();
  });

  it("marks a signal that matches the vendor list, without claiming it named the device", () => {
    renderCard({ signals: [signal({ inferred: true })] });
    const badge = screen.getByText("Matches vendor list");
    expect(badge).toBeInTheDocument();
    // Naming is first-writer-wins against an empty manufacturer, so a match is
    // evidence about the device — not proof of where its name came from. A
    // device can even match two vendors at once.
    expect(badge.getAttribute("title")).not.toMatch(/named the device/);
  });

  it("does not mark a signal that only records what the device said", () => {
    renderCard({
      signals: [signal({ kind: "dhcp_hostname", value: "some-host" })],
    });
    expect(screen.queryByText("Matches vendor list")).not.toBeInTheDocument();
  });

  it("says what identifying will do before it is pressed", () => {
    // The probe is the one identification mechanism that sends traffic to the
    // admin's own device (ADR 0025 §5), so the consent has to be informed.
    renderCard();
    expect(
      screen.getByText(/contacts this device directly/),
    ).toBeInTheDocument();
    expect(screen.getByText(/never does this on its own/)).toBeInTheDocument();
  });

  it("probes the device when the action is pressed", async () => {
    mutateAsync.mockResolvedValue(probeResult());
    renderCard();

    await userEvent.click(
      screen.getByRole("button", { name: "Identify this device" }),
    );

    expect(mutateAsync).toHaveBeenCalledWith("device-1");
  });

  it("says plainly when nothing answered", async () => {
    // The outcome is not persisted, so without this the card would look
    // unchanged and the button would look broken.
    mutateAsync.mockResolvedValue(probeResult({ answering_ports: [] }));
    renderCard();

    await userEvent.click(
      screen.getByRole("button", { name: "Identify this device" }),
    );

    await waitFor(() =>
      expect(screen.getByText(/No known ports answered/)).toBeInTheDocument(),
    );
  });

  it("stays quiet when ports did answer", async () => {
    mutateAsync.mockResolvedValue(probeResult());
    renderCard();

    await userEvent.click(
      screen.getByRole("button", { name: "Identify this device" }),
    );

    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
    expect(
      screen.queryByText(/No known ports answered/),
    ).not.toBeInTheDocument();
  });

  it("shows the pending label while probing", () => {
    renderCard({ isPending: true });
    expect(screen.getByRole("button", { name: "Identifying…" })).toBeDisabled();
  });

  it("claims no result when the probe is refused", async () => {
    // The daemon returns 409 for a device that left the network after this page
    // loaded — reachable in normal use, since the button's enabled state comes
    // from `last_seen` at render time and the detail query does not poll. The
    // card must not then assert anything about what answered.
    //
    // The other half of that path — `runProbe` swallowing the rethrow so it is
    // not an unhandled rejection — is not assertable here: jsdom does not fire
    // `unhandledrejection` for it. It follows the same try/catch convention as
    // `PrivateDnsCard.handleGrant`.
    mutateAsync.mockRejectedValue(
      Object.assign(new Error("conflict"), { status: 409 }),
    );
    renderCard();

    await userEvent.click(
      screen.getByRole("button", { name: "Identify this device" }),
    );

    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
    expect(
      screen.queryByText(/No known ports answered/),
    ).not.toBeInTheDocument();
  });

  it("disables the action with an explanation when the device is off the network", async () => {
    // The daemon refuses a probe for a departed device — its last known address
    // may since have been handed to someone else, and the vendor would be
    // recorded against the wrong device row permanently.
    renderCard({ online: false });

    const button = screen.getByRole("button", { name: "Identify this device" });
    expect(button).toBeDisabled();
    expect(button.getAttribute("title")).toMatch(
      /not currently on the network/,
    );
    expect(mutateAsync).not.toHaveBeenCalled();
  });
});
