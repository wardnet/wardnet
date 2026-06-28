import { Button } from "@wardnet/web";
import {
  AlertModal,
  AlertModalAction,
  AlertModalCancel,
  AlertModalContent,
  AlertModalDescription,
  AlertModalFooter,
  AlertModalHeader,
  AlertModalTitle,
} from "@wardnet/web";

interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: string;
  confirmLabel?: string;
  onConfirm: () => void;
  destructive?: boolean;
}

/** Reusable confirmation dialog for destructive actions. */
export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel = "Confirm",
  onConfirm,
  destructive = true,
}: ConfirmDialogProps) {
  return (
    <AlertModal open={open} onOpenChange={onOpenChange}>
      <AlertModalContent>
        <AlertModalHeader>
          <AlertModalTitle>{title}</AlertModalTitle>
          <AlertModalDescription>{description}</AlertModalDescription>
        </AlertModalHeader>
        <AlertModalFooter>
          <AlertModalCancel asChild>
            <Button variant="outline" data-testid="confirm-dialog-cancel">
              Cancel
            </Button>
          </AlertModalCancel>
          <AlertModalAction asChild>
            <Button
              variant={destructive ? "destructive" : "default"}
              onClick={onConfirm}
              data-testid="confirm-dialog-confirm"
            >
              {confirmLabel}
            </Button>
          </AlertModalAction>
        </AlertModalFooter>
      </AlertModalContent>
    </AlertModal>
  );
}
