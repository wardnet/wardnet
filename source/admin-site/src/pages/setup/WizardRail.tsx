import { clsx } from "clsx";
import { Logo, Text } from "@wardnet/web";
import type { WizardStep } from "@wardnet/js";
import { WIZARD_STEPS, canRewindTo, ordinalOf } from "@/pages/setup/steps";
import { useWizardNav } from "@/pages/setup/useWizardNav";

/**
 * The wizard's persistent chrome: logo plus step navigation.
 *
 * Wide (>600px container): a vertical list of all steps with
 * done / current / upcoming treatments. Done rows are buttons that
 * rewind the wizard (the daemon allows rewinds down to the floor);
 * the current and upcoming rows are inert — forward jumps are never
 * offered.
 *
 * Narrow (≤600px container): the list is replaced by a slim progress
 * bar with a "Step n of N · Label" caption.
 */
export function WizardRail({ current }: { current: WizardStep }) {
  const nav = useWizardNav(current);
  const currentIdx = ordinalOf(current);
  const percent = Math.round(((currentIdx + 1) / WIZARD_STEPS.length) * 100);
  const currentLabel = WIZARD_STEPS[currentIdx]?.label ?? "";

  return (
    <>
      <Logo height={22} variant="light" className="shrink-0" />

      {/* Desktop rail list */}
      <nav
        aria-label="Setup steps"
        className="mt-[22px] hidden @min-[601px]:block"
      >
        <Text
          as="p"
          size="2xs"
          weight="bold"
          className="mb-3.5 tracking-[.14em] text-ink-3 uppercase"
        >
          Setup
        </Text>
        <ol className="flex flex-col gap-0.5">
          {WIZARD_STEPS.map((step, i) => {
            const isCurrent = i === currentIdx;
            const isDone = i < currentIdx || current === "completed";
            const clickable = canRewindTo(current, step.id);
            return (
              <li key={step.id}>
                <button
                  type="button"
                  disabled={!clickable || nav.isPending}
                  onClick={clickable ? () => nav.goTo(step.id) : undefined}
                  aria-current={isCurrent ? "step" : undefined}
                  data-testid={`setup-rail-${step.id}`}
                  className={clsx(
                    "flex w-full items-center gap-[11px] rounded-md px-2.5 py-[9px] text-left transition-colors duration-(--duration-snap)",
                    isCurrent && "bg-elev text-ink shadow-card",
                    !isCurrent && isDone && "text-ink-2",
                    !isCurrent && !isDone && "text-ink-3",
                    clickable && "cursor-pointer hover:bg-ink/5",
                    !clickable && "cursor-default",
                  )}
                >
                  <Text
                    as="span"
                    size="2xs"
                    weight="semibold"
                    className={clsx(
                      "flex size-[22px] shrink-0 items-center justify-center rounded-full",
                      isCurrent && "bg-accent text-white",
                      !isCurrent &&
                        isDone &&
                        "bg-accent-soft text-accent-soft-ink",
                      !isCurrent &&
                        !isDone &&
                        "border border-line-strong bg-elev text-ink-3",
                    )}
                  >
                    {i + 1}
                  </Text>
                  <Text as="span" size="sm" weight="medium">
                    {step.label}
                  </Text>
                </button>
              </li>
            );
          })}
        </ol>
      </nav>

      {/* Mobile progress bar */}
      <div className="mt-4 @min-[601px]:hidden">
        <div className="h-1.5 w-full overflow-hidden rounded-full bg-sunken">
          <div
            className="h-full rounded-full bg-accent transition-[width] duration-[400ms] ease-[cubic-bezier(.4,0,.2,1)]"
            style={{ width: `${percent}%` }}
          />
        </div>
        <div className="mt-3 flex items-baseline justify-between">
          <Text as="p" size="sm" weight="medium" className="text-ink-3">
            Step {currentIdx + 1} of {WIZARD_STEPS.length} · {currentLabel}
          </Text>
          <Text
            as="p"
            size="xs"
            weight="semibold"
            className="mono text-accent-soft-ink"
          >
            {percent}%
          </Text>
        </div>
      </div>
    </>
  );
}
