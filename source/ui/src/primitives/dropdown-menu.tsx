import * as React from "react";
import { DropdownMenu as RadixDropdownMenu } from "radix-ui";
import { clsx } from "clsx";
import s from "./overlay.module.css";

type DropdownMenuProps = React.ComponentProps<typeof RadixDropdownMenu.Root>;

function DropdownMenu(props: DropdownMenuProps) {
  return <RadixDropdownMenu.Root {...props} />;
}

type DropdownMenuTriggerProps = React.ComponentProps<
  typeof RadixDropdownMenu.Trigger
>;

function DropdownMenuTrigger(props: DropdownMenuTriggerProps) {
  return <RadixDropdownMenu.Trigger {...props} />;
}

type DropdownMenuContentProps = React.ComponentProps<
  typeof RadixDropdownMenu.Content
> & {
  container?: React.ComponentProps<
    typeof RadixDropdownMenu.Portal
  >["container"];
};

function DropdownMenuContent({
  className,
  container,
  align = "end",
  sideOffset = 4,
  ...props
}: DropdownMenuContentProps) {
  return (
    <RadixDropdownMenu.Portal container={container}>
      <RadixDropdownMenu.Content
        align={align}
        sideOffset={sideOffset}
        className={clsx(s.popover, className)}
        {...props}
      />
    </RadixDropdownMenu.Portal>
  );
}

type DropdownMenuItemProps = React.ComponentProps<
  typeof RadixDropdownMenu.Item
> & {
  /** `destructive` items render in danger color and should be grouped at the
   *  bottom of the menu, separated from safe items by a {@link DropdownMenuSeparator}. */
  variant?: "default" | "destructive";
};

function DropdownMenuItem({
  className,
  variant = "default",
  ...props
}: DropdownMenuItemProps) {
  return (
    <RadixDropdownMenu.Item
      data-variant={variant}
      className={clsx(s.menuItem, className)}
      {...props}
    />
  );
}

type DropdownMenuSeparatorProps = React.ComponentProps<
  typeof RadixDropdownMenu.Separator
>;

function DropdownMenuSeparator({
  className,
  ...props
}: DropdownMenuSeparatorProps) {
  return (
    <RadixDropdownMenu.Separator
      className={clsx(s.menuSeparator, className)}
      {...props}
    />
  );
}

export {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
};
export type {
  DropdownMenuProps,
  DropdownMenuTriggerProps,
  DropdownMenuContentProps,
  DropdownMenuItemProps,
  DropdownMenuSeparatorProps,
};
