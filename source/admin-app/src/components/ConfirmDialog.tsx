import { Drawer, DrawerContent, DrawerClose, Text } from "@wardnet/web";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
  title: string;
  description: string;
  confirmLabel?: string;
  variant?: "danger" | "warn";
}

export function ConfirmDialog({
  open,
  onOpenChange,
  onConfirm,
  title,
  description,
  confirmLabel = "Confirm",
  variant = "danger",
}: Props) {
  return (
    <Drawer open={open} onOpenChange={onOpenChange}>
      <DrawerContent
          side="bottom"
          data-testid="confirm-dialog"
          onPointerDownOutside={(e) => e.preventDefault()}
          onEscapeKeyDown={(e) => e.preventDefault()}
        >
        {/* Drag handle */}
        <div className="flex justify-center pb-1 pt-3">
          <div className="h-1 w-10 rounded-full bg-line" />
        </div>

        {/* Copy */}
        <div className="px-6 pb-6 text-center">
          <Text as="h2" size="xl" weight="bold" className="tracking-tight text-ink">{title}</Text>
          <Text as="p" size="base" className="mt-2 leading-relaxed text-ink-3">{description}</Text>
        </div>

        {/* Buttons */}
        <div className="flex flex-col gap-2 px-4 pb-10">
          <button
            data-testid="confirm-dialog-confirm"
            onClick={onConfirm}
            className={`w-full rounded-2xl py-[15px] text-[15px] font-semibold tracking-tight ${
              variant === "danger"
                ? "bg-danger text-white active:opacity-85"
                : "bg-warn text-warn-soft-ink active:opacity-85"
            }`}
          >
            {confirmLabel}
          </button>
          <DrawerClose className="w-full rounded-2xl py-[15px] text-[15px] font-medium text-ink-2 active:bg-sunken">
            Cancel
          </DrawerClose>
        </div>
      </DrawerContent>
    </Drawer>
  );
}
