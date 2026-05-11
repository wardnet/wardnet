import * as React from "react";
import { Switch } from "radix-ui";
import { clsx } from "clsx";

type ToggleProps = React.ComponentProps<typeof Switch.Root>;

function Toggle({ className, ...props }: ToggleProps) {
  return <Switch.Root className={clsx("toggle", className)} {...props} />;
}

export { Toggle };
export type { ToggleProps };
