import * as React from "react";
import { Label as RadixLabel } from "radix-ui";
import { clsx } from "clsx";
import s from "./field.module.css";

type LabelProps = React.ComponentProps<typeof RadixLabel.Root>;

function Label({ className, ...props }: LabelProps) {
  return <RadixLabel.Root className={clsx(s.label, className)} {...props} />;
}

export { Label };
export type { LabelProps };
