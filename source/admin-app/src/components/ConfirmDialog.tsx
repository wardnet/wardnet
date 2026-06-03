import { Drawer, DrawerContent, DrawerClose } from "@wardnet/forge-web/drawer";

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
          onPointerDownOutside={(e) => e.preventDefault()}
          onEscapeKeyDown={(e) => e.preventDefault()}
        >
        {/* Drag handle */}
        <div className="flex justify-center pb-1 pt-3">
          <div className="h-1 w-10 rounded-full bg-line" />
        </div>

        {/* Copy */}
        <div className="px-6 pb-6 text-center">
          <h2 className="text-[19px] font-bold tracking-tight text-ink">{title}</h2>
          <p className="mt-2 text-[14px] leading-relaxed text-ink-3">{description}</p>
        </div>

        {/* Buttons */}
        <div className="flex flex-col gap-2 px-4 pb-10">
          <button
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
