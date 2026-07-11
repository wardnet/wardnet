import type { ReactNode } from "react";
import {
  AlertModalHeader,
  AlertModalTitle,
  AlertModalDescription,
} from "@wardnet/ui";

interface AlertModalTitleBlockProps {
  title: ReactNode;
  /** Optional supporting copy, rendered muted under the title. */
  description?: ReactNode;
}

/**
 * Stacked title + description for an `AlertModal`'s header — the confirm/alert
 * counterpart of {@link ModalTitleBlock}.
 *
 * The design-system `AlertModalHeader` is a flex **row**, so dropping an
 * `AlertModalTitle` and `AlertModalDescription` into it directly lays them out
 * side by side (a squeezed title on the left, description on the right). This
 * wraps them in a full-width column so the description sits under the title,
 * and mutes it. Use it for every confirm/alert dialog so the header never
 * regresses.
 */
export function AlertModalTitleBlock({
  title,
  description,
}: AlertModalTitleBlockProps) {
  return (
    <AlertModalHeader>
      <div className="flex w-full min-w-0 flex-col gap-1">
        <AlertModalTitle>{title}</AlertModalTitle>
        {description && (
          <AlertModalDescription className="text-sm leading-relaxed text-ink-3">
            {description}
          </AlertModalDescription>
        )}
      </div>
    </AlertModalHeader>
  );
}
