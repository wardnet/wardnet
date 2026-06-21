import * as React from "react";
import { clsx } from "clsx";
import s from "./stat-tile.module.css";

type StatTileProps = Omit<React.ComponentProps<"div">, "children"> & {
  /** Uppercase 12px label rendered in the `.stat__label` slot. */
  label: React.ReactNode;
  /** Headline numeric value rendered in `.stat__value`. */
  value: React.ReactNode;
  /** Optional inline unit (e.g. "%", "MB"); rendered as `.unit` inside `.stat__value`. */
  unit?: React.ReactNode;
  /** Optional secondary line rendered in `.stat__sub`. */
  sub?: React.ReactNode;
  /** Optional 0–100 percentage rendered as a `.bar` row beneath the value. */
  bar?: number;
  /** Optional sparkline / chart rendered in the bottom-pinned `.stat__spark` slot. */
  spark?: React.ReactNode;
  /** Optional pill / badge rendered in the top-right `.stat__pill` slot. */
  pill?: React.ReactNode;
};

// Why a slot-based prop API: the spark prop accepts a ReactNode rather than
// raw values + color. This keeps StatTile platform-thin (no chart runtime
// coupling) and lets consumers render the Sparkline primitive — which lands
// in slice 2b — without churn here.
function StatTile({
  className,
  label,
  value,
  unit,
  sub,
  bar,
  spark,
  pill,
  ...props
}: StatTileProps) {
  return (
    <div data-slot="stat-tile" className={clsx(s.stat, className)} {...props}>
      {pill !== undefined && <div className={s.pill}>{pill}</div>}
      <div className={clsx("t-label", s.label)}>{label}</div>
      <div className={clsx("t-metric", s.value)}>
        {value}
        {unit !== undefined && (
          <span className={clsx("t-metric-unit", s.unit)}>{unit}</span>
        )}
      </div>
      {sub !== undefined && <div className={s.sub}>{sub}</div>}
      {bar !== undefined && (
        <div className={s.bar}>
          <span style={{ width: `${bar}%` }} />
        </div>
      )}
      {spark !== undefined && <div className={s.spark}>{spark}</div>}
    </div>
  );
}

export { StatTile };
export type { StatTileProps };
