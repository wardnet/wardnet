import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { DeviceSignal } from "@wardnet/js";

import { DeviceIdentificationCard } from "@/components/features/DeviceIdentificationCard";
import { renderWithProviders } from "../../test-utils";

function signal(overrides: Partial<DeviceSignal> = {}): DeviceSignal {
  return {
    kind: "mdns_service",
    value: "_govee._tcp",
    inferred: false,
    observed_at: "2026-08-05T10:00:00Z",
    ...overrides,
  };
}

describe("DeviceIdentificationCard", () => {
  it("explains an empty list instead of showing a bare blank", () => {
    // A device seen only by ARP has no signals, and that must not read as a
    // Wardnet failure — the confusion issue #1099 was filed about.
    renderWithProviders(<DeviceIdentificationCard signals={[]} />);
    expect(screen.getByText(/Nothing observed yet/)).toBeInTheDocument();
    // The copy must not promise observation Wardnet does not yet perform:
    // mDNS browsing and port probing are #1115/#1116, not shipped here.
    expect(screen.queryByText(/announces a service/)).not.toBeInTheDocument();
  });

  it("groups signals under a per-kind heading", () => {
    renderWithProviders(
      <DeviceIdentificationCard
        signals={[
          signal({ value: "_govee._tcp" }),
          signal({ kind: "dhcp_hostname", value: "govee-lamp" }),
        ]}
      />,
    );
    expect(screen.getByText("Announced services (mDNS)")).toBeInTheDocument();
    expect(screen.getByText("Hostname (DHCP)")).toBeInTheDocument();
    expect(screen.getByText("_govee._tcp")).toBeInTheDocument();
    expect(screen.getByText("govee-lamp")).toBeInTheDocument();
  });

  it("marks a signal that matches the vendor list, without claiming it named the device", () => {
    renderWithProviders(
      <DeviceIdentificationCard signals={[signal({ inferred: true })]} />,
    );
    const badge = screen.getByText("Matches vendor list");
    expect(badge).toBeInTheDocument();
    // Naming is first-writer-wins against an empty manufacturer, so a match is
    // evidence about the device — not proof of where its name came from. A
    // device can even match two vendors at once.
    expect(badge.getAttribute("title")).not.toMatch(/named the device/);
  });

  it("does not mark a signal that only records what the device said", () => {
    renderWithProviders(
      <DeviceIdentificationCard
        signals={[signal({ kind: "dhcp_hostname", value: "some-host" })]}
      />,
    );
    expect(screen.queryByText("Matches vendor list")).not.toBeInTheDocument();
  });
});
