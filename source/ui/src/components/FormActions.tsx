import type { ReactNode } from "react";
import { clsx } from "clsx";
import { Button } from "../primitives/button";
import { CardFooter } from "../primitives/card";
import s from "./FormActions.module.css";

interface FormActionsProps {
  /** Primary action — rendered rightmost in the accent (or danger) colour. */
  primaryLabel: ReactNode;
  onPrimary: () => void;
  primaryVariant?: "default" | "destructive";
  /**
   * Secondary action — rendered to the LEFT of the primary as a ghost button.
   * Omit to render only the primary action.
   */
  secondaryLabel?: ReactNode;
  onSecondary?: () => void;
  /** Disables both buttons (e.g. while a mutation is in flight). */
  disabled?: boolean;
  className?: string;
}

/**
 * Canonical action footer used at the bottom of a Card form: a ghost secondary
 * on the left and the primary action rightmost, right-aligned, inside a
 * `CardFooter` so it picks up the standard sunken footer background + top
 * border. Reserves the accent colour for the single primary action.
 *
 * Render as a direct child of `<Card>` (a sibling of `<CardContent>`).
 */
export function FormActions({
  primaryLabel,
  onPrimary,
  primaryVariant = "default",
  secondaryLabel,
  onSecondary,
  disabled,
  className,
}: FormActionsProps) {
  return (
    <CardFooter className={clsx(s.actions, className)}>
      {secondaryLabel != null && (
        <Button variant="ghost" onClick={onSecondary} disabled={disabled}>
          {secondaryLabel}
        </Button>
      )}
      <Button variant={primaryVariant} onClick={onPrimary} disabled={disabled}>
        {primaryLabel}
      </Button>
    </CardFooter>
  );
}
