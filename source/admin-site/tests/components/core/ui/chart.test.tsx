import { renderHook, render } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";
import { LineChart } from "recharts";
import {
  ChartContainer,
  useChart,
  type ChartConfig,
} from "@/components/core/ui/chart";

beforeAll(() => {
  // recharts' ResponsiveContainer + ChartContainer rely on ResizeObserver and
  // element measurement, neither of which jsdom implements. Report a positive
  // size so ResponsiveContainer renders its child.
  globalThis.ResizeObserver = class {
    constructor(private cb: ResizeObserverCallback) {}
    observe(el: Element) {
      this.cb(
        [
          {
            target: el,
            contentRect: { width: 300, height: 150 },
          } as unknown as ResizeObserverEntry,
        ],
        this as unknown as ResizeObserver,
      );
    }
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  Element.prototype.getBoundingClientRect = () =>
    ({
      width: 300,
      height: 150,
      top: 0,
      left: 0,
      right: 300,
      bottom: 150,
    }) as DOMRect;
});

describe("useChart", () => {
  it("throws when used outside a ChartContainer", () => {
    expect(() => renderHook(() => useChart())).toThrow(
      /useChart must be used inside/,
    );
  });
});

describe("ChartContainer", () => {
  const config: ChartConfig = {
    rx: { label: "RX", color: "var(--chart-1)" },
    tx: { label: "TX" },
  };

  it("renders the chart wrapper and exposes color CSS vars", () => {
    const { container } = render(
      <ChartContainer config={config} className="h-40">
        <LineChart data={[]} />
      </ChartContainer>,
    );
    const wrapper = container.querySelector(
      '[data-slot="chart"]',
    ) as HTMLElement;
    expect(wrapper).toBeInTheDocument();
    expect(wrapper.className).toContain("chart");
    // Only entries with a color produce a CSS var.
    expect(wrapper.style.getPropertyValue("--color-rx")).toBe("var(--chart-1)");
    expect(wrapper.style.getPropertyValue("--color-tx")).toBe("");
  });

  it("provides config to consumers via useChart", () => {
    function Consumer(props: { width?: number; height?: number }) {
      void props;
      const { config: c } = useChart();
      return <span>{c.rx.label}</span>;
    }
    // ResponsiveContainer injects width/height and renders its single child;
    // Consumer is a plain component so it renders regardless of measured size.
    const { getByText } = render(
      <ChartContainer config={config}>
        <Consumer />
      </ChartContainer>,
    );
    expect(getByText("RX")).toBeInTheDocument();
  });
});
