import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import { clsx } from "clsx";
import s from "./pill.module.css";

const pillVariants = cva(s.pill, {
  variants: {
    variant: {
      ok: s.ok,
      warn: s.warn,
      down: s.down,
      info: s.info,
      ghost: s.ghost,
    },
  },
});

type PillProps = React.ComponentProps<"span"> &
  VariantProps<typeof pillVariants> & {
    asChild?: boolean;
  };

function Pill({ className, variant, asChild = false, ...props }: PillProps) {
  const Comp = asChild ? Slot.Root : "span";
  return (
    <Comp className={clsx(pillVariants({ variant }), className)} {...props} />
  );
}

export { Pill };
export type { PillProps };
