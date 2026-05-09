import * as React from "react";
import * as RechartsPrimitive from "recharts";

import { cn } from "@/lib/utils";

/**
 * Minimal port of the shadcn `<ChartContainer>` wrapper.
 *
 * We deliberately keep this small: it provides a themed container that
 * sets CSS custom properties for each series colour, plus a thin
 * `ResponsiveContainer` so children fill the parent. Tooltips and
 * legends are rendered with recharts primitives directly — recharts'
 * built-ins look fine in our theme without an extra wrapper layer.
 */
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
          "[&_.recharts-cartesian-axis-tick_text]:fill-muted-foreground",
          "[&_.recharts-cartesian-grid_line]:stroke-border",
          "[&_.recharts-tooltip-cursor]:stroke-border",
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
