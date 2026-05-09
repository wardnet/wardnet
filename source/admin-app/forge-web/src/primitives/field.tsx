import * as React from "react";
import { clsx } from "clsx";

import { Label } from "./label";

type FieldProps = {
  label?: React.ReactNode;
  htmlFor?: string;
  /** Use when the control is referenced via `aria-labelledby` (custom widgets
   *  like ProfileToggleList that don't pair with an `htmlFor` target). */
  labelId?: string;
  help?: React.ReactNode;
  /** Layout. `column` (default) stacks label / control / help vertically.
   *  `row` puts the label-and-help block on the left and the control on the
   *  right — used for settings-style rows where the control (toggle / select)
   *  sits inline with its label. */
  direction?: "column" | "row";
  editing?: boolean;
  value?: React.ReactNode;
  children?: React.ReactNode;
  className?: string;
};

function Field({
  label,
  htmlFor,
  labelId,
  help,
  direction = "column",
  editing = true,
  value,
  children,
  className,
}: FieldProps) {
  const showRead = !editing && value !== undefined;
  const labelEl =
    label !== undefined ? (
      <Label htmlFor={htmlFor} id={labelId}>
        {label}
      </Label>
    ) : null;
  const helpEl =
    help !== undefined && help !== false ? <p className="field-help">{help}</p> : null;
  const controlEl = showRead ? <span className="field-value">{value}</span> : children;

  if (direction === "row") {
    return (
      <div className={clsx("field", className)} data-direction="row">
        <div className="field-text">
          {labelEl}
          {helpEl}
        </div>
        {controlEl}
      </div>
    );
  }

  return (
    <div className={clsx("field", className)}>
      {labelEl}
      {controlEl}
      {helpEl}
    </div>
  );
}

export { Field };
export type { FieldProps };
