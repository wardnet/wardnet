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

export function ChartContainer({ config, className, children, ...props }: ChartContainerProps) {
  // Recharts SVG props (stroke/fill) take string values, so we expose
  // each series colour as a per-instance CSS custom property
  // (`--color-<key>`) on the wrapper. Consumers reference those via
  // `var(--color-<key>)` on `<Line stroke=...>` and friends. These vars
  // are scoped to this container — they are *not* the legacy shadcn
  // `--color-*` aliases.
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
        className={cn(
          "flex aspect-video w-full justify-center text-xs",
          "[&_.recharts-cartesian-axis-tick_text]:fill-ink-3",
          "[&_.recharts-cartesian-grid_line]:stroke-line",
          "[&_.recharts-tooltip-cursor]:stroke-line",
          className,
        )}
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
