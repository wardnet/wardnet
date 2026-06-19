import * as React from "react";
import { clsx } from "clsx";
import s from "./field.module.css";

type TextareaProps = React.ComponentProps<"textarea">;

function Textarea({ className, ...props }: TextareaProps) {
  return <textarea className={clsx(s.textarea, className)} {...props} />;
}

export { Textarea };
export type { TextareaProps };
