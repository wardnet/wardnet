import { useAdvanceWizard } from "@wardnet/web";
import type { WizardMode, WizardStep } from "@wardnet/js";
import { canRewindTo, nextOf, prevOf } from "@/pages/setup/steps";

/**
 * Navigation actions for one wizard step.
 *
 * Wraps `useAdvanceWizard` with the canonical step registry so step
 * components never hard-code their neighbour's id. The caller passes its
 * own step id — no context required, so steps stay renderable (and
 * testable) standalone.
 */
export function useWizardNav(step: WizardStep) {
  const advance = useAdvanceWizard();

  /** Advance to the next step in the canonical order. */
  const goNext = (opts?: { wizard_mode?: WizardMode }) => {
    const next = nextOf(step);
    if (!next) return;
    advance.mutate(
      opts?.wizard_mode
        ? { to_step: next, wizard_mode: opts.wizard_mode }
        : { to_step: next },
    );
  };

  /** Rewind to the previous step, if the floor allows it. */
  const goBack = () => {
    const prev = prevOf(step);
    if (prev) goTo(prev);
  };

  /** Rewind to an arbitrary earlier step (rail rows, review Edit links). */
  const goTo = (target: WizardStep) => {
    if (!canRewindTo(step, target)) return;
    advance.mutate({ to_step: target });
  };

  return {
    goNext,
    goBack,
    goTo,
    isPending: advance.isPending,
    isError: advance.isError,
  };
}
