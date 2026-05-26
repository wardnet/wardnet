import * as React from "react";
import * as RechartsPrimitive from "recharts";

import { cn } from "@/lib/utils";

export type ChartConfig = Record<
  string,
  {
    label: string;
    color?: string;
  }
>;

const ChartContext = React.createContext<{ config: ChartConfig } | null>(null);

export function useChart(): { config: ChartConfig } {
  const ctx = React.useContext(ChartContext);
  if (!ctx) {
    throw new Error("useChart must be used inside <ChartContainer>");
  }
  return ctx;
}

// `ref` is intentionally omitted from the public surface: the wrapper
// div is owned by ChartContainer's internal ResizeObserver, and exposing
// a forwarded ref would let consumers stomp on it. If a consumer ever
// needs imperative access, switch to `forwardRef` + `useImperativeHandle`
// rather than re-adding `ref` here.
export interface ChartContainerProps
  extends Omit<React.ComponentProps<"div">, "ref"> {
  config: ChartConfig;
  children: React.ComponentProps<typeof RechartsPrimitive.ResponsiveContainer>["children"];
}

/**
 * Forge §10 chart wrapper. The `.chart` Forge class owns the visual
 * contract — horizontal hairlines only, mono Y-axis labels, no
 * vertical grid, soft-fill area, card-styled tooltip — so this
 * component just attaches the class and a ResponsiveContainer.
 *
 * Per-instance series colours are exposed as `--color-<key>` CSS
 * variables on the wrapper so consumers can write
 * `<Line stroke="var(--color-rx)">` and have Recharts' SVG props
 * (which require string values, not class lookups) resolve through
 * the chartConfig. This is NOT the legacy shadcn `--color-*` alias
 * bridge — those were drained in earlier slices. The Forge chart
 * palette (`--chart-1` … `--chart-4`) is defined globally in
 * `styles.css`; consumers point a chartConfig entry at
 * `var(--chart-1)` and the indirection bridges the named series
 * (e.g. "rx") to the concrete colour.
 *
 * We measure the wrapper div ourselves with ResizeObserver and pass
 * fixed numeric width/height to ResponsiveContainer. Recharts only
 * creates its SizeDetectorContainer (and emits the "width(-1) and
 * height(-1)" warning) when width/height are percentages — numeric
 * values take the static calculation path and never warn.
 */
export function ChartContainer({ config, className, children, ...props }: ChartContainerProps) {
  const cssVars = React.useMemo(() => {
    const out: Record<string, string> = {};
    for (const [key, entry] of Object.entries(config)) {
      if (entry.color) {
        out[`--color-${key}`] = entry.color;
      }
    }
    return out;
  }, [config]);

  const containerRef = React.useRef<HTMLDivElement>(null);
  // Seed with (1, 1) so the chart renders immediately on first paint
  // without a flash of empty space and without the recharts warning.
  const [dims, setDims] = React.useState({ width: 1, height: 1 });

  React.useLayoutEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const update = (w: number, h: number) =>
      setDims((prev) => (prev.width === w && prev.height === h ? prev : { width: w, height: h }));
    const { width, height } = el.getBoundingClientRect();
    update(width, height);
    const observer = new ResizeObserver(([entry]) => {
      const { width: w, height: h } = entry.contentRect;
      update(w, h);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  return (
    <ChartContext.Provider value={{ config }}>
      <div
        data-slot="chart"
        className={cn("chart", className)}
        style={cssVars as React.CSSProperties}
        {...props}
        ref={containerRef}
      >
        <RechartsPrimitive.ResponsiveContainer width={dims.width} height={dims.height}>
          {children}
        </RechartsPrimitive.ResponsiveContainer>
      </div>
    </ChartContext.Provider>
  );
}

export const ChartTooltip = RechartsPrimitive.Tooltip;
export const ChartLegend = RechartsPrimitive.Legend;
