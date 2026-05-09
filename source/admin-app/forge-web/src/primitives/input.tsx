import * as React from "react";
import { clsx } from "clsx";

type InputProps = React.ComponentProps<"input">;

function Input({ className, ...props }: InputProps) {
  return <input className={clsx("input", className)} {...props} />;
}

export { Input };
export type { InputProps };
