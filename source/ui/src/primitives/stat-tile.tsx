import * as React from "react";
import { clsx } from "clsx";

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
    <div data-slot="stat-tile" className={clsx("stat", className)} {...props}>
      {pill !== undefined && <div className="stat__pill">{pill}</div>}
      <div className="stat__label">{label}</div>
      <div className="stat__value">
        {value}
        {unit !== undefined && <span className="unit">{unit}</span>}
      </div>
      {sub !== undefined && <div className="stat__sub">{sub}</div>}
      {bar !== undefined && (
        <div className="bar">
          <span style={{ width: `${bar}%` }} />
        </div>
      )}
      {spark !== undefined && <div className="stat__spark">{spark}</div>}
    </div>
  );
}

export { StatTile };
export type { StatTileProps };
