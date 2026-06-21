import * as React from "react";
import { clsx } from "clsx";
import s from "./sparkline.module.css";

type SparklineProps = Omit<
  React.ComponentProps<"svg">,
  "children" | "values"
> & {
  /**
   * Sample points rendered left-to-right. Normalised to a 0..1 viewBox so
   * the host container fully owns width and height; passing 1, 30, or 300
   * samples all render the same total width.
   */
  values: number[];
  /**
   * CSS colour for the stroke (and area wash, if `area` is true). Accepts
   * any value valid for `--spark-color` — typically `var(--accent)` /
   * `var(--info)` / `var(--warn)`. Defaults to `var(--accent)` per §10
   * rule 01 (Download = `--accent`).
   */
  color?: string;
  /**
   * Render a soft-fill area beneath the line at low opacity. Mirrors the
   * §13 area-chart precedent (Recharts `recharts-area-area` at
   * `fill-opacity: 0.12`). Defaults to `true` because every Forge studio
   * mock renders the wash.
   */
  area?: boolean;
};

// Why a thin inline-SVG primitive instead of Recharts: the studio mock
// (design-system.html §10 around line 1407) renders Sparkline as a single
// <polyline>/<path> with no axes, no tooltip, no legend. Recharts would
// pull in a chart runtime + ResponsiveContainer for what is two SVG
// elements. The component stays tree-shakeable and free of chart
// dependencies; consumers needing axes use the §10 ChartContainer
// (recharts-backed) instead.
function Sparkline({
  className,
  values,
  color,
  area = true,
  style,
  ...props
}: SparklineProps) {
  // 100x40 viewBox keeps numbers human-readable in devtools and matches
  // the 2.5:1 aspect of the stat-tile spark slot (.stat__spark is 36px
  // tall, ~90–120px wide). preserveAspectRatio="none" lets the parent
  // box freely stretch the curve, matching the §10 throughput chart's
  // `width:"100%"` host pattern.
  const W = 100;
  const H = 40;

  if (values.length === 0) {
    return (
      <svg
        data-slot="sparkline"
        viewBox={`0 0 ${W} ${H}`}
        preserveAspectRatio="none"
        className={clsx(s.sparkline, className)}
        style={
          color !== undefined
            ? ({ ...style, "--spark-color": color } as React.CSSProperties)
            : style
        }
        {...props}
      />
    );
  }

  const min = Math.min(...values);
  const max = Math.max(...values);
  // Guard against flat series — a 0-range domain would divide-by-zero
  // and render NaN coords. Pin the line to the vertical mid-band so
  // "no movement" reads as a quiet horizontal stroke.
  const range = max - min || 1;
  const stepX = values.length > 1 ? W / (values.length - 1) : 0;
  const points = values.map((v, i) => {
    const x = i * stepX;
    // Inset 1px top + bottom so the 1.5px stroke isn't clipped at the
    // viewBox edge once preserveAspectRatio="none" stretches the path.
    const y = H - 1 - ((v - min) / range) * (H - 2);
    return `${x.toFixed(2)},${y.toFixed(2)}`;
  });
  const lineD = `M ${points.join(" L ")}`;
  const areaD = `${lineD} L ${(values.length > 1 ? W : 0).toFixed(2)},${H} L 0,${H} Z`;

  return (
    <svg
      data-slot="sparkline"
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="none"
      className={clsx(s.sparkline, className)}
      style={
        color !== undefined
          ? ({ ...style, "--spark-color": color } as React.CSSProperties)
          : style
      }
      {...props}
    >
      {area && <path d={areaD} className={s.area} />}
      <path d={lineD} className={s.line} />
    </svg>
  );
}

export { Sparkline };
export type { SparklineProps };
