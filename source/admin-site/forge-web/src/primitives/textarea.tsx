import * as React from "react";
import { clsx } from "clsx";

type TextareaProps = React.ComponentProps<"textarea">;

function Textarea({ className, ...props }: TextareaProps) {
  return <textarea className={clsx("textarea", className)} {...props} />;
}

export { Textarea };
export type { TextareaProps };
