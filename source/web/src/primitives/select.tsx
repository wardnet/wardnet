import * as React from "react";
import { Select as RadixSelect } from "radix-ui";
import { clsx } from "clsx";

type SelectProps = React.ComponentProps<typeof RadixSelect.Root>;

function Select(props: SelectProps) {
  return <RadixSelect.Root {...props} />;
}

type SelectTriggerProps = React.ComponentProps<typeof RadixSelect.Trigger>;

function SelectTrigger({ className, children, ...props }: SelectTriggerProps) {
  return (
    <RadixSelect.Trigger
      className={clsx("select-trigger", className)}
      {...props}
    >
      {children}
    </RadixSelect.Trigger>
  );
}

type SelectValueProps = React.ComponentProps<typeof RadixSelect.Value>;

function SelectValue(props: SelectValueProps) {
  return <RadixSelect.Value {...props} />;
}

type SelectContentProps = React.ComponentProps<typeof RadixSelect.Content> & {
  container?: React.ComponentProps<typeof RadixSelect.Portal>["container"];
};

function SelectContent({
  className,
  container,
  children,
  position = "popper",
  align = "start",
  sideOffset = 4,
  ...props
}: SelectContentProps) {
  return (
    <RadixSelect.Portal container={container}>
      <RadixSelect.Content
        className={clsx("popover", "select-content", className)}
        position={position}
        align={align}
        sideOffset={sideOffset}
        {...props}
      >
        <RadixSelect.Viewport>{children}</RadixSelect.Viewport>
      </RadixSelect.Content>
    </RadixSelect.Portal>
  );
}

type SelectItemProps = React.ComponentProps<typeof RadixSelect.Item>;

function SelectItem({ className, children, ...props }: SelectItemProps) {
  return (
    <RadixSelect.Item className={clsx("menu-item", className)} {...props}>
      <RadixSelect.ItemText>{children}</RadixSelect.ItemText>
    </RadixSelect.Item>
  );
}

export { Select, SelectTrigger, SelectValue, SelectContent, SelectItem };
export type {
  SelectProps,
  SelectTriggerProps,
  SelectValueProps,
  SelectContentProps,
  SelectItemProps,
};
