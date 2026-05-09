import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import { clsx } from "clsx";

const buttonVariants = cva("btn", {
  variants: {
    variant: {
      default: "btn--primary",
      outline: "",
      secondary: "",
      ghost: "btn--ghost",
      destructive: "btn--danger",
      tertiary: "btn--ghost",
    },
    size: {
      default: "",
      sm: "btn--sm",
      icon: "btn--icon",
      "icon-sm": "btn--icon btn--sm",
    },
  },
  defaultVariants: {
    variant: "default",
    size: "default",
  },
});

type ButtonProps = React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean;
  };

function Button({
  className,
  variant,
  size,
  asChild = false,
  ...props
}: ButtonProps) {
  const Comp = asChild ? Slot.Root : "button";
  return (
    <Comp
      className={clsx(buttonVariants({ variant, size }), className)}
      {...props}
    />
  );
}

export { Button };
export type { ButtonProps };
