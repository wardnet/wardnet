import * as React from "react";
import { Switch } from "radix-ui";
import { clsx } from "clsx";
import s from "./toggle.module.css";

type ToggleProps = React.ComponentProps<typeof Switch.Root>;

function Toggle({ className, ...props }: ToggleProps) {
  return <Switch.Root className={clsx(s.toggle, className)} {...props} />;
}

export { Toggle };
export type { ToggleProps };
