import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { ChartContainer, type ChartConfig } from "@/components/core/ui/chart";
import { cn } from "@wardnet/web";
import type { ComponentProps } from "react";
import type { ResponsiveContainer } from "recharts";

interface Props {
  config: ChartConfig;
  className?: string;
  /** Whether to render the Reset zoom affordance — driven by `useChartZoom().isZoomed`. */
  isZoomed: boolean;
  onResetZoom: () => void;
  children: ComponentProps<typeof ResponsiveContainer>["children"];
}

/**
 * Drop-in replacement for `<ChartContainer>` for charts that opt into
 * drag-to-zoom via `useChartZoom`. Adds an overlaid Reset button while
 * a zoom selection is active — the chart event wiring itself stays on
 * the consumer so each chart can pass the hook's `chartProps` to its
 * own Recharts root.
 */
export function ZoomableChartContainer({
  config,
  className,
  isZoomed,
  onResetZoom,
  children,
}: Props) {
  return (
    <div className="relative select-none [&_svg]:outline-none">
      <ChartContainer config={config} className={className}>
        {children}
      </ChartContainer>
      {isZoomed && (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={onResetZoom}
          className={cn(
            "absolute right-2 top-2 z-10 h-6 px-2 text-ink-3",
            "hover:text-ink-1",
          )}
        >
          <Text as="span" size="2xs">
            Reset zoom
          </Text>
        </Button>
      )}
    </div>
  );
}
