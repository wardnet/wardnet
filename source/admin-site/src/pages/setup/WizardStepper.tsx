import { clsx } from "clsx";
import { Text } from "@wardnet/web";
import type { WizardStep } from "@wardnet/js";

const STEPS: { id: WizardStep; label: string }[] = [
  { id: "admin", label: "Admin" },
  { id: "network", label: "Network" },
  { id: "dhcp", label: "DHCP" },
  { id: "router_mac", label: "Router" },
  { id: "tunnel", label: "Tunnel" },
  { id: "policy", label: "Policy" },
  { id: "remote_access", label: "HTTPS" },
  { id: "completed", label: "Done" },
];

function ordinal(step: WizardStep): number {
  return STEPS.findIndex((s) => s.id === step);
}

/**
 * Compact step indicator that fits the auth card's max-w-sm width.
 *
 * Renders as a row of small numbered dots connected by progress lines,
 * with the current step's label spelled out underneath. Showing every
 * label inline would overflow the 384 px card on the 7-step wizard.
 */
export function WizardStepper({ current }: { current: WizardStep }) {
  const currentIndex = ordinal(current);
  const currentStep = STEPS[currentIndex];

  return (
    <div className="mb-5 flex flex-col gap-2">
      <ol className="flex w-full items-center">
        {STEPS.map((step, i) => {
          const isPast = i < currentIndex;
          const isCurrent = i === currentIndex;
          return (
            <li
              key={step.id}
              className="flex flex-1 items-center last:flex-none"
            >
              <Text
                size="2xs"
                weight="semibold"
                aria-label={step.label}
                aria-current={isCurrent ? "step" : undefined}
                className={clsx(
                  "flex h-6 w-6 shrink-0 items-center justify-center rounded-full border",
                  isCurrent && "border-accent bg-accent text-accent-ink",
                  !isCurrent &&
                    isPast &&
                    "border-accent bg-accent/15 text-accent",
                  !isCurrent && !isPast && "border-line text-ink-3",
                )}
              >
                {i + 1}
              </Text>
              {i < STEPS.length - 1 && (
                <span
                  className={clsx(
                    "mx-1 h-px flex-1",
                    isPast ? "bg-accent/40" : "bg-line",
                  )}
                />
              )}
            </li>
          );
        })}
      </ol>
      <Text as="p" size="xs" className="text-ink-3">
        Step {currentIndex + 1} of {STEPS.length} — {currentStep?.label}
      </Text>
    </div>
  );
}
