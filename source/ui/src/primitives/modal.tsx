import * as React from "react";
import { Dialog } from "radix-ui";
import { clsx } from "clsx";
import s from "./overlay.module.css";

type ModalProps = React.ComponentProps<typeof Dialog.Root>;

function Modal(props: ModalProps) {
  return <Dialog.Root {...props} />;
}

type ModalTriggerProps = React.ComponentProps<typeof Dialog.Trigger>;

function ModalTrigger(props: ModalTriggerProps) {
  return <Dialog.Trigger {...props} />;
}

type ModalContentProps = React.ComponentProps<typeof Dialog.Content> & {
  /** Optional Portal container override. */
  container?: React.ComponentProps<typeof Dialog.Portal>["container"];
};

function ModalContent({
  className,
  container,
  children,
  ...props
}: ModalContentProps) {
  return (
    <Dialog.Portal container={container}>
      <Dialog.Overlay className={s.scrim} />
      <Dialog.Content className={clsx(s.modal, className)} {...props}>
        {children}
      </Dialog.Content>
    </Dialog.Portal>
  );
}

function ModalHeader({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={clsx(s.modalHead, className)} {...props} />;
}

type ModalTitleProps = Omit<
  React.ComponentProps<typeof Dialog.Title>,
  "asChild"
> &
  React.ComponentProps<"h3">;

function ModalTitle({ className, ...props }: ModalTitleProps) {
  return (
    <Dialog.Title asChild>
      <h3 className={className} {...props} />
    </Dialog.Title>
  );
}

type ModalDescriptionProps = React.ComponentProps<typeof Dialog.Description>;

function ModalDescription(props: ModalDescriptionProps) {
  return <Dialog.Description {...props} />;
}

function ModalBody({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={clsx(s.modalBody, className)} {...props} />;
}

function ModalFooter({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={clsx(s.modalFoot, className)} {...props} />;
}

type ModalCloseProps = React.ComponentProps<typeof Dialog.Close>;

function ModalClose(props: ModalCloseProps) {
  return <Dialog.Close {...props} />;
}

export {
  Modal,
  ModalTrigger,
  ModalContent,
  ModalHeader,
  ModalTitle,
  ModalDescription,
  ModalBody,
  ModalFooter,
  ModalClose,
};
export type {
  ModalProps,
  ModalTriggerProps,
  ModalContentProps,
  ModalTitleProps,
  ModalDescriptionProps,
  ModalCloseProps,
};
