import { describe, expect, it } from "vitest";
import { DashboardUsageBar } from "@/components/compound/DashboardUsageBar";
import { renderWithProviders } from "../../test-utils";

function fill(container: HTMLElement): HTMLElement {
  return container.querySelector(".bar > span") as HTMLElement;
}

describe("DashboardUsageBar", () => {
  it("picks distinct colors for the three thresholds", () => {
    const { container: low } = renderWithProviders(
      <DashboardUsageBar value={30} />,
    );
    const { container: mid } = renderWithProviders(
      <DashboardUsageBar value={60} />,
    );
    const { container: high } = renderWithProviders(
      <DashboardUsageBar value={90} />,
    );

    expect(fill(low).style.width).toBe("30%");
    expect(fill(mid).style.width).toBe("60%");
    expect(fill(high).style.width).toBe("90%");

    const accent = fill(low).style.background;
    const warn = fill(mid).style.background;
    const danger = fill(high).style.background;
    expect(accent).toBeTruthy();
    expect(new Set([accent, warn, danger]).size).toBe(3);
  });

  it("clamps values above 100 and below 0", () => {
    const { container: hi } = renderWithProviders(
      <DashboardUsageBar value={150} />,
    );
    expect(fill(hi).style.width).toBe("100%");

    const { container: lo } = renderWithProviders(
      <DashboardUsageBar value={-20} />,
    );
    expect(fill(lo).style.width).toBe("0%");
    // 150 clamps to 100 → danger; -20 clamps to 0 → accent: distinct colors.
    expect(fill(lo).style.background).not.toBe(fill(hi).style.background);
    expect(fill(lo).style.background).toBeTruthy();
  });
});
