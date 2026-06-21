import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import { clsx } from "clsx";
import s from "./button.module.css";

const buttonVariants = cva(s.btn, {
  variants: {
    variant: {
      default: s.primary,
      outline: "",
      secondary: "",
      ghost: s.ghost,
      destructive: s.danger,
      tertiary: s.ghost,
    },
    size: {
      default: "",
      sm: s.sm,
      icon: s.icon,
      "icon-sm": clsx(s.icon, s.sm),
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
