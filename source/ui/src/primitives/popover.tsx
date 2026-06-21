import * as React from "react";
import { Popover as RadixPopover } from "radix-ui";
import { clsx } from "clsx";
import s from "./overlay.module.css";

type PopoverProps = React.ComponentProps<typeof RadixPopover.Root>;

function Popover(props: PopoverProps) {
  return <RadixPopover.Root {...props} />;
}

type PopoverTriggerProps = React.ComponentProps<typeof RadixPopover.Trigger>;

function PopoverTrigger(props: PopoverTriggerProps) {
  return <RadixPopover.Trigger {...props} />;
}

type PopoverContentProps = React.ComponentProps<typeof RadixPopover.Content> & {
  container?: React.ComponentProps<typeof RadixPopover.Portal>["container"];
};

function PopoverContent({
  className,
  container,
  align = "center",
  sideOffset = 4,
  ...props
}: PopoverContentProps) {
  return (
    <RadixPopover.Portal container={container}>
      <RadixPopover.Content
        align={align}
        sideOffset={sideOffset}
        className={clsx(s.popover, className)}
        {...props}
      />
    </RadixPopover.Portal>
  );
}

export { Popover, PopoverTrigger, PopoverContent };
export type { PopoverProps, PopoverTriggerProps, PopoverContentProps };
