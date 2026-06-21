import * as React from "react";
import { Eye, EyeOff } from "lucide-react";
import { clsx } from "clsx";
import s from "./field.module.css";

type InputProps = React.ComponentProps<"input">;

/**
 * Forge `.input`. For `type="password"` we additionally render a small
 * eye toggle on the right so the user can briefly reveal the value —
 * a familiar web convention. The toggle swaps the rendered `type`
 * between `password` and `text` while keeping the same value/onChange
 * wiring intact (so any controlled state and form validators see the
 * value either way).
 *
 * The reveal button is `tabIndex={-1}` so it doesn't sit between the
 * password field and the submit button in the keyboard flow.
 */
function Input({ className, type, ...props }: InputProps) {
  const [revealed, setRevealed] = React.useState(false);

  if (type === "password") {
    return (
      <div className={s.inputWrap}>
        <input
          {...props}
          type={revealed ? "text" : "password"}
          className={clsx(s.input, s.withReveal, className)}
        />
        <button
          type="button"
          className={s.inputReveal}
          onClick={() => setRevealed((v) => !v)}
          aria-label={revealed ? "Hide password" : "Show password"}
          aria-pressed={revealed}
          tabIndex={-1}
        >
          {revealed ? <EyeOff aria-hidden /> : <Eye aria-hidden />}
        </button>
      </div>
    );
  }

  return <input className={clsx(s.input, className)} type={type} {...props} />;
}

export { Input };
export type { InputProps };
