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

export interface ChartContainerProps extends React.ComponentProps<"div"> {
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

  return (
    <ChartContext.Provider value={{ config }}>
      <div
        data-slot="chart"
        className={cn("chart", className)}
        style={cssVars as React.CSSProperties}
        {...props}
      >
        <RechartsPrimitive.ResponsiveContainer>{children}</RechartsPrimitive.ResponsiveContainer>
      </div>
    </ChartContext.Provider>
  );
}

export const ChartTooltip = RechartsPrimitive.Tooltip;
export const ChartLegend = RechartsPrimitive.Legend;
