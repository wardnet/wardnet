import { useEffect, useState, type ReactNode } from "react";
import { Loader2Icon, AlertTriangleIcon } from "lucide-react";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  AlertModal,
  AlertModalCancel,
  AlertModalContent,
  AlertModalDescription,
  AlertModalFooter,
  AlertModalHeader,
  AlertModalTitle,
} from "@wardnet/web";

/**
 * Three-state machine that drives the visual shell of a long-running
 * operation modal. Domain dialogs (restart, shutdown, …) collapse
 * their richer phase enums into one of these three buckets and pass
 * domain-specific copy/icons through the content slots below.
 */
export type GenericPhase = "in-progress" | "success" | "failed";

interface ProgressDialogProps {
  /** Whether to render the dialog at all. */
  open: boolean;
  /** Coarse-grained lifecycle bucket. */
  phase: GenericPhase;
  /** Heading shown next to the spinner / status icon. */
  title: string;
  /** Optional grey copy below the title (in-progress + failed). */
  description?: ReactNode;
  /** UTC ms timestamp the operation started, for the elapsed counter. */
  startedAt: number | null;
  /** Error message rendered in the failed-phase callout. */
  errorMessage: string | null;
  /** Icon shown on success (e.g. <CheckCircle2Icon/>, <PowerOffIcon/>). */
  successIcon: ReactNode;
  /** Heading on success (replaces `title`). */
  successTitle: string;
  /** Optional grey copy below `successTitle`. */
  successDescription?: ReactNode;
  /** Optional CTA rendered alongside the dismiss button on success. */
  successAction?: ReactNode;
  /** Called when the operator dismisses the modal (only valid in terminal phases). */
  onDismiss: () => void;
  /** Soft cap (seconds) shown alongside the elapsed counter. */
  timeoutSeconds?: number;
}

/**
 * Generic progress modal for daemon-side long-running operations.
 *
 * Used by [`RestartProgressDialog`](../features/RestartProgressDialog.tsx)
 * and [`ShutdownProgressDialog`](../features/ShutdownProgressDialog.tsx)
 * to share the full-screen-overlay shell. The two domain wrappers map
 * their own phase enums into the coarse `GenericPhase` and supply
 * domain-specific copy and icons.
 *
 * Layout per phase:
 * - `in-progress`: spinner + `title`, optional `description`, live elapsed
 *   counter. No footer — the dialog is intentionally inescapable while
 *   the daemon is mid-flight.
 * - `success`: `successIcon` + `successTitle`, optional `successDescription`,
 *   optional `successAction` plus a Dismiss button.
 * - `failed`: warning icon + `title`, `errorMessage` rendered in a
 *   `bg-danger-soft` / `text-danger-soft-ink` callout, Dismiss button.
 */
export function ProgressDialog({
  open,
  phase,
  title,
  description,
  startedAt,
  errorMessage,
  successIcon,
  successTitle,
  successDescription,
  successAction,
  onDismiss,
  timeoutSeconds = 45,
}: ProgressDialogProps) {
  const elapsed = useElapsedSeconds(
    open && phase === "in-progress" ? startedAt : null,
  );
  const terminal = phase !== "in-progress";

  return (
    <AlertModal
      open={open}
      onOpenChange={(next) => {
        if (!next && terminal) onDismiss();
      }}
    >
      <AlertModalContent data-testid="progress-dialog">
        <AlertModalHeader>
          <AlertModalTitle
            className="flex items-center gap-2"
            data-testid="progress-title"
          >
            {phase === "in-progress" && (
              <Loader2Icon className="h-5 w-5 animate-spin text-ink-3" />
            )}
            {phase === "success" && successIcon}
            {phase === "failed" && (
              <AlertTriangleIcon className="h-5 w-5 text-danger" />
            )}
            {phase === "success" ? successTitle : title}
          </AlertModalTitle>
          {phase === "success" && successDescription && (
            <AlertModalDescription>{successDescription}</AlertModalDescription>
          )}
          {phase === "in-progress" && description && (
            <AlertModalDescription>{description}</AlertModalDescription>
          )}
          {phase === "failed" && description && (
            <AlertModalDescription>{description}</AlertModalDescription>
          )}
        </AlertModalHeader>

        {phase === "in-progress" && (
          <Text as="div" size="xs" className="text-ink-3">
            Elapsed: {elapsed}s (times out at {timeoutSeconds}s).
          </Text>
        )}

        {phase === "failed" && errorMessage && (
          <Text
            as="div"
            size="sm"
            className="rounded-md bg-danger-soft px-3 py-2 text-danger-soft-ink"
          >
            {errorMessage}
          </Text>
        )}

        {terminal && (
          <AlertModalFooter>
            {phase === "success" && successAction}
            <AlertModalCancel asChild>
              <Button
                variant="outline"
                onClick={onDismiss}
                data-testid="progress-dismiss"
              >
                Dismiss
              </Button>
            </AlertModalCancel>
          </AlertModalFooter>
        )}
      </AlertModalContent>
    </AlertModal>
  );
}

/** Live-updating elapsed-seconds counter, paused while `startedAt` is null. */
function useElapsedSeconds(startedAt: number | null): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (startedAt === null) return;
    const id = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(id);
  }, [startedAt]);
  return startedAt === null
    ? 0
    : Math.max(0, Math.floor((now - startedAt) / 1000));
}
