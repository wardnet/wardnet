import type { WizardStep } from "@wardnet/js";

const STEPS: { id: WizardStep; label: string }[] = [
  { id: "admin", label: "Admin" },
  { id: "network", label: "Network" },
  { id: "dhcp", label: "DHCP" },
  { id: "router_mac", label: "Router" },
  { id: "tunnel", label: "Tunnel" },
  { id: "policy", label: "Policy" },
  { id: "completed", label: "Done" },
];

function ordinal(step: WizardStep): number {
  return STEPS.findIndex((s) => s.id === step);
}

export function WizardStepper({ current }: { current: WizardStep }) {
  const currentIndex = ordinal(current);

  return (
    <ol className="mb-6 flex w-full items-center gap-2 text-xs">
      {STEPS.map((step, i) => {
        const isPast = i < currentIndex;
        const isCurrent = i === currentIndex;
        return (
          <li key={step.id} className="flex flex-1 items-center gap-2">
            <span
              className={
                "flex h-7 w-7 items-center justify-center rounded-full border text-xs font-semibold " +
                (isCurrent
                  ? "border-primary bg-primary text-primary-foreground"
                  : isPast
                    ? "border-primary bg-primary/15 text-primary"
                    : "border-muted-foreground/30 text-muted-foreground")
              }
            >
              {i + 1}
            </span>
            <span
              className={
                isCurrent
                  ? "font-medium text-foreground"
                  : isPast
                    ? "text-muted-foreground"
                    : "text-muted-foreground/60"
              }
            >
              {step.label}
            </span>
            {i < STEPS.length - 1 && (
              <span
                className={"h-px flex-1 " + (isPast ? "bg-primary/40" : "bg-muted-foreground/20")}
              />
            )}
          </li>
        );
      })}
    </ol>
  );
}
