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

  it("marks the signal that matched the vendor list", () => {
    renderWithProviders(
      <DeviceIdentificationCard signals={[signal({ inferred: true })]} />,
    );
    expect(screen.getByText("Matched vendor list")).toBeInTheDocument();
  });

  it("does not mark a signal that only records what the device said", () => {
    renderWithProviders(
      <DeviceIdentificationCard
        signals={[signal({ kind: "dhcp_hostname", value: "some-host" })]}
      />,
    );
    expect(screen.queryByText("Matched vendor list")).not.toBeInTheDocument();
  });
});
