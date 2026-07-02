import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TabBar } from "@/components/TabBar";
import { renderWithProviders } from "../test-utils";

describe("TabBar", () => {
  it("renders all five nav tabs with their routes", () => {
    renderWithProviders(<TabBar />);
    for (const label of ["Home", "Devices", "Tunnels", "DNS", "System"]) {
      expect(screen.getByTestId(`tab-${label.toLowerCase()}`)).toBeInTheDocument();
    }
    expect(screen.getByTestId("tab-home")).toHaveAttribute("href", "/");
    expect(screen.getByTestId("tab-devices")).toHaveAttribute("href", "/devices");
  });
});
