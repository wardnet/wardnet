import type { ReactNode } from "react";
import { ModalHeader, ModalTitle, ModalDescription } from "@wardnet/ui";

interface ModalTitleBlockProps {
  title: ReactNode;
  /** Optional supporting copy, rendered muted under the title. */
  description?: ReactNode;
}

/**
 * Stacked title + description for a `Modal`'s header.
 *
 * The design-system `ModalHeader` is a flex **row** (a title beside an
 * optional trailing action), so dropping a `ModalTitle` and `ModalDescription`
 * into it directly lays them out side by side — a squeezed title on the left,
 * the description on the right. This wraps them in a full-width column so the
 * description sits under the title, and mutes the description text. Reuse it
 * for any informational modal that needs a title + description.
 */
export function ModalTitleBlock({ title, description }: ModalTitleBlockProps) {
  return (
    <ModalHeader>
      <div className="flex w-full min-w-0 flex-col gap-1">
        <ModalTitle>{title}</ModalTitle>
        {description && (
          <ModalDescription className="text-sm leading-relaxed text-ink-3">
            {description}
          </ModalDescription>
        )}
      </div>
    </ModalHeader>
  );
}
