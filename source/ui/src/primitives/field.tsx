import * as React from "react";
import { clsx } from "clsx";

import { Label } from "./label";
import { useFormContext } from "./form";
import s from "./field.module.css";

type FieldProps = {
  label?: React.ReactNode;
  htmlFor?: string;
  /** Field name. When present and the field is nested inside a `<Form>`,
   *  validation errors registered by `<Validator name="...">` flow into
   *  this field automatically. */
  name?: string;
  /** Use when the control is referenced via `aria-labelledby` (custom widgets
   *  like ProfileToggleList that don't pair with an `htmlFor` target). */
  labelId?: string;
  help?: React.ReactNode;
  /** Inline validation message(s). Accepts a single ReactNode (back-compat
   *  with one-off uses) or an array of messages — each renders on its own
   *  `.field-error` line. When set, the wrapper carries
   *  `data-invalid="true"` and the help text is suppressed.
   *  When the field is in a Form, errors come from `FormContext` via
   *  the `name` prop — pass `error` only to override. */
  error?: React.ReactNode | React.ReactNode[];
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
  name,
  labelId,
  help,
  error,
  direction = "column",
  editing = true,
  value,
  children,
  className,
}: FieldProps) {
  const formCtx = useFormContext();
  const showRead = !editing && value !== undefined;

  // Resolve which messages to display. An explicit `error` prop wins;
  // otherwise pull from FormContext via the field name (if any).
  const errorMessages: React.ReactNode[] = React.useMemo(() => {
    if (error !== undefined && error !== null && error !== false) {
      return Array.isArray(error) ? error : [error];
    }
    if (name && formCtx?.errors[name]?.length) {
      return formCtx.errors[name];
    }
    return [];
  }, [error, name, formCtx]);

  const labelEl =
    label !== undefined ? (
      <Label htmlFor={htmlFor} id={labelId}>
        {label}
      </Label>
    ) : null;
  const errorEl =
    errorMessages.length > 0 ? (
      <>
        {errorMessages.map((msg, i) => (
          <p key={i} className={s.fieldError}>
            {msg}
          </p>
        ))}
      </>
    ) : null;
  // Help is suppressed when an error is shown — the error message
  // replaces it so we don't stack two helper lines under the control.
  const helpEl =
    !errorEl && help !== undefined && help !== false ? (
      <p className={s.fieldHelp}>{help}</p>
    ) : null;
  const controlEl = showRead ? (
    <span className={s.fieldValue}>{value}</span>
  ) : (
    children
  );
  const invalidProps = errorEl ? { "data-invalid": "true" as const } : {};

  if (direction === "row") {
    return (
      <div
        className={clsx(s.field, className)}
        data-direction="row"
        {...invalidProps}
      >
        <div className={s.fieldText}>
          {labelEl}
          {helpEl}
          {errorEl}
        </div>
        {controlEl}
      </div>
    );
  }

  return (
    <div className={clsx(s.field, className)} {...invalidProps}>
      {labelEl}
      {controlEl}
      {helpEl}
      {errorEl}
    </div>
  );
}

export { Field };
export type { FieldProps };
