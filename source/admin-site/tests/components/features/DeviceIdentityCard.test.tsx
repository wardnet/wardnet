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

  it("names why a manufacturer is missing instead of showing a bare dash", () => {
    renderWithProviders(
      <DeviceIdentityCard
        device={makeDevice({ hostname: null, manufacturer: null })}
      />,
    );

    // A bare "-" left the admin unable to tell a Wardnet failure from a vendor
    // who took a private IEEE listing, which is the dead end in issue #1099.
    expect(screen.getByText("Unknown manufacturer")).toBeInTheDocument();
    expect(screen.getByText("Unknown manufacturer")).toHaveAttribute("title");
    // Hostname keeps its dash — its absence needs no explaining.
    expect(screen.getByText("-")).toBeInTheDocument();
  });

  it("hedges a curated vendor guess rather than asserting it", () => {
    renderWithProviders(
      <DeviceIdentityCard
        device={makeDevice({
          manufacturer: "Govee",
          manufacturer_source: "catalog",
        })}
      />,
    );

    // The IEEE listing for this block is deliberately private, so the name is
    // our inference and must not read as a registered fact.
    expect(screen.getByText("Likely Govee")).toBeInTheDocument();
  });

  it("badges a randomized MAC on the address, not as a manufacturer", () => {
    renderWithProviders(
      <DeviceIdentityCard
        device={makeDevice({ is_randomized: true, manufacturer: null })}
      />,
    );

    expect(screen.getByText("Randomized")).toBeInTheDocument();
    // "Randomized MAC" used to be written into the manufacturer column, which
    // conflated how a device presents itself with who built it.
    expect(screen.queryByText("Randomized MAC")).not.toBeInTheDocument();
  });
});
