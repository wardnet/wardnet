import * as React from "react";
import { clsx } from "clsx";

import { Label } from "./label";

type FieldProps = {
  label: React.ReactNode;
  htmlFor?: string;
  help?: React.ReactNode;
  editing?: boolean;
  value?: React.ReactNode;
  children?: React.ReactNode;
  className?: string;
};

function Field({
  label,
  htmlFor,
  help,
  editing = true,
  value,
  children,
  className,
}: FieldProps) {
  const showRead = !editing && value !== undefined;
  return (
    <div className={clsx("field", className)}>
      <Label htmlFor={htmlFor}>{label}</Label>
      {showRead ? <span className="field-value">{value}</span> : children}
      {help !== undefined && <p className="field-help">{help}</p>}
    </div>
  );
}

export { Field };
export type { FieldProps };
