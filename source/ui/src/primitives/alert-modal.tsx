import * as React from "react";
import { AlertDialog } from "radix-ui";
import { clsx } from "clsx";
import s from "./overlay.module.css";

type AlertModalProps = React.ComponentProps<typeof AlertDialog.Root>;

function AlertModal(props: AlertModalProps) {
  return <AlertDialog.Root {...props} />;
}

type AlertModalTriggerProps = React.ComponentProps<typeof AlertDialog.Trigger>;

function AlertModalTrigger(props: AlertModalTriggerProps) {
  return <AlertDialog.Trigger {...props} />;
}

type AlertModalContentProps = React.ComponentProps<
  typeof AlertDialog.Content
> & {
  /** Optional Portal container override. */
  container?: React.ComponentProps<typeof AlertDialog.Portal>["container"];
};

function AlertModalContent({
  className,
  container,
  children,
  ...props
}: AlertModalContentProps) {
  return (
    <AlertDialog.Portal container={container}>
      <AlertDialog.Overlay className={s.scrim} />
      <AlertDialog.Content className={clsx(s.modal, className)} {...props}>
        {children}
      </AlertDialog.Content>
    </AlertDialog.Portal>
  );
}

function AlertModalHeader({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return <div className={clsx(s.modalHead, className)} {...props} />;
}

type AlertModalTitleProps = Omit<
  React.ComponentProps<typeof AlertDialog.Title>,
  "asChild"
> &
  React.ComponentProps<"h3">;

function AlertModalTitle({ className, ...props }: AlertModalTitleProps) {
  return (
    <AlertDialog.Title asChild>
      <h3 className={className} {...props} />
    </AlertDialog.Title>
  );
}

type AlertModalDescriptionProps = React.ComponentProps<
  typeof AlertDialog.Description
>;

function AlertModalDescription(props: AlertModalDescriptionProps) {
  return <AlertDialog.Description {...props} />;
}

function AlertModalBody({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={clsx(s.modalBody, className)} {...props} />;
}

function AlertModalFooter({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return <div className={clsx(s.modalFoot, className)} {...props} />;
}

type AlertModalActionProps = React.ComponentProps<typeof AlertDialog.Action>;

function AlertModalAction(props: AlertModalActionProps) {
  return <AlertDialog.Action {...props} />;
}

type AlertModalCancelProps = React.ComponentProps<typeof AlertDialog.Cancel>;

function AlertModalCancel(props: AlertModalCancelProps) {
  return <AlertDialog.Cancel {...props} />;
}

export {
  AlertModal,
  AlertModalTrigger,
  AlertModalContent,
  AlertModalHeader,
  AlertModalTitle,
  AlertModalDescription,
  AlertModalBody,
  AlertModalFooter,
  AlertModalAction,
  AlertModalCancel,
};
export type {
  AlertModalProps,
  AlertModalTriggerProps,
  AlertModalContentProps,
  AlertModalTitleProps,
  AlertModalDescriptionProps,
  AlertModalActionProps,
  AlertModalCancelProps,
};
