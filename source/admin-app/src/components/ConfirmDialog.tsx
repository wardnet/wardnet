import { Button } from "@wardnet/forge-web/button";
import {
  AlertModal,
  AlertModalAction,
  AlertModalCancel,
  AlertModalContent,
  AlertModalDescription,
  AlertModalFooter,
  AlertModalHeader,
  AlertModalTitle,
} from "@wardnet/forge-web/alert-modal";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
  title: string;
  description: string;
  confirmLabel?: string;
  variant?: "danger" | "warn";
  icon?: React.ReactNode;
}

export function ConfirmDialog({
  open,
  onOpenChange,
  onConfirm,
  title,
  description,
  confirmLabel = "Confirm",
  variant = "danger",
  icon,
}: Props) {
  return (
    <AlertModal open={open} onOpenChange={onOpenChange}>
      <AlertModalContent>
        <AlertModalHeader>
          {icon && (
            <div
              className={`mb-3 flex h-12 w-12 items-center justify-center rounded-xl ${
                variant === "danger"
                  ? "bg-danger-soft text-danger-soft-ink"
                  : "bg-warn-soft text-warn-soft-ink"
              }`}
            >
              {icon}
            </div>
          )}
          <AlertModalTitle>{title}</AlertModalTitle>
          <AlertModalDescription>{description}</AlertModalDescription>
        </AlertModalHeader>
        <AlertModalFooter>
          <AlertModalCancel asChild>
            <Button variant="outline">Cancel</Button>
          </AlertModalCancel>
          <AlertModalAction asChild>
            <Button
              variant={variant === "danger" ? "destructive" : "default"}
              onClick={onConfirm}
            >
              {confirmLabel}
            </Button>
          </AlertModalAction>
        </AlertModalFooter>
      </AlertModalContent>
    </AlertModal>
  );
}
