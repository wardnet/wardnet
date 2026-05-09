import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import { clsx } from "clsx";

const pillVariants = cva("pill", {
  variants: {
    variant: {
      ok: "pill--ok",
      warn: "pill--warn",
      down: "pill--down",
      info: "pill--info",
      ghost: "pill--ghost",
    },
  },
});

type PillProps = React.ComponentProps<"span"> &
  VariantProps<typeof pillVariants> & {
    asChild?: boolean;
  };

function Pill({
  className,
  variant,
  asChild = false,
  ...props
}: PillProps) {
  const Comp = asChild ? Slot.Root : "span";
  return (
    <Comp className={clsx(pillVariants({ variant }), className)} {...props} />
  );
}

export { Pill };
export type { PillProps };
