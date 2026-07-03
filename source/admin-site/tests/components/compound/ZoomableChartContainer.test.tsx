import { describe, expect, it, vi, beforeAll } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// jsdom lacks ResizeObserver, which the ChartContainer wrapper uses.
beforeAll(() => {
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as never;
});
import type { ChartConfig } from "@/components/core/ui/chart";
import { ZoomableChartContainer } from "@/components/compound/ZoomableChartContainer";
import { renderWithProviders } from "../../test-utils";

const config: ChartConfig = { rx: { label: "RX", color: "var(--chart-1)" } };

describe("ZoomableChartContainer", () => {
  it("renders the chart wrapper and no reset button when not zoomed", () => {
    const { container } = renderWithProviders(
      <ZoomableChartContainer
        config={config}
        isZoomed={false}
        onResetZoom={vi.fn()}
      >
        <div data-testid="chart-child">child</div>
      </ZoomableChartContainer>,
    );
    expect(container.querySelector('[data-slot="chart"]')).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Reset zoom" }),
    ).not.toBeInTheDocument();
  });

  it("renders a reset button when zoomed and calls onResetZoom", async () => {
    const user = userEvent.setup();
    const onResetZoom = vi.fn();
    renderWithProviders(
      <ZoomableChartContainer
        config={config}
        isZoomed={true}
        onResetZoom={onResetZoom}
      >
        <div data-testid="chart-child">child</div>
      </ZoomableChartContainer>,
    );
    await user.click(screen.getByRole("button", { name: "Reset zoom" }));
    expect(onResetZoom).toHaveBeenCalledTimes(1);
  });
});
