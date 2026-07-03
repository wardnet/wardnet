import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DeviceIdentityCard } from "@/components/features/DeviceIdentityCard";
import { makeDevice, renderWithProviders } from "../../test-utils";

describe("DeviceIdentityCard", () => {
  it("renders MAC, hostname and manufacturer when present", () => {
    renderWithProviders(
      <DeviceIdentityCard
        device={makeDevice({
          mac: "AA:BB:CC:DD:EE:FF",
          hostname: "living-room-tv",
          manufacturer: "Acme Corp",
        })}
      />,
    );

    expect(screen.getByText("AA:BB:CC:DD:EE:FF")).toBeInTheDocument();
    expect(screen.getByText("living-room-tv")).toBeInTheDocument();
    expect(screen.getByText("Acme Corp")).toBeInTheDocument();
    expect(screen.getByText("Identity")).toBeInTheDocument();
  });

  it("falls back to an em dash for missing hostname and manufacturer", () => {
    renderWithProviders(
      <DeviceIdentityCard
        device={makeDevice({ hostname: null, manufacturer: null })}
      />,
    );

    // Both hostname and manufacturer render the em dash placeholder.
    expect(screen.getAllByText("—").length).toBeGreaterThanOrEqual(2);
  });
});
