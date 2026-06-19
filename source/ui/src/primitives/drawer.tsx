import * as React from "react";
import { Dialog } from "radix-ui";
import { clsx } from "clsx";
import s from "./overlay.module.css";

type DrawerProps = React.ComponentProps<typeof Dialog.Root>;

function Drawer(props: DrawerProps) {
  return <Dialog.Root {...props} />;
}

type DrawerTriggerProps = React.ComponentProps<typeof Dialog.Trigger>;

function DrawerTrigger(props: DrawerTriggerProps) {
  return <Dialog.Trigger {...props} />;
}

type DrawerContentProps = React.ComponentProps<typeof Dialog.Content> & {
  side?: "left" | "right" | "bottom";
  container?: React.ComponentProps<typeof Dialog.Portal>["container"];
};

function DrawerContent({
  className,
  container,
  side = "left",
  children,
  ...props
}: DrawerContentProps) {
  return (
    <Dialog.Portal container={container}>
      <Dialog.Overlay className={s.scrim} />
      <Dialog.Content
        className={clsx(s.drawer, className)}
        data-side={side}
        {...props}
      >
        {children}
      </Dialog.Content>
    </Dialog.Portal>
  );
}

type DrawerTitleProps = Omit<
  React.ComponentProps<typeof Dialog.Title>,
  "asChild"
> &
  React.ComponentProps<"h3">;

function DrawerTitle({ className, ...props }: DrawerTitleProps) {
  return (
    <Dialog.Title asChild>
      <h3 className={className} {...props} />
    </Dialog.Title>
  );
}

type DrawerDescriptionProps = React.ComponentProps<typeof Dialog.Description>;

function DrawerDescription(props: DrawerDescriptionProps) {
  return <Dialog.Description {...props} />;
}

type DrawerCloseProps = React.ComponentProps<typeof Dialog.Close>;

function DrawerClose(props: DrawerCloseProps) {
  return <Dialog.Close {...props} />;
}

export {
  Drawer,
  DrawerTrigger,
  DrawerContent,
  DrawerTitle,
  DrawerDescription,
  DrawerClose,
};
export type {
  DrawerProps,
  DrawerTriggerProps,
  DrawerContentProps,
  DrawerTitleProps,
  DrawerDescriptionProps,
  DrawerCloseProps,
};
