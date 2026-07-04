import { useState, type ReactNode } from "react";
import { ArrowLeftIcon } from "lucide-react";
import { Text } from "@wardnet/web";
import type { WizardStep } from "@wardnet/js";
import { WizardRail } from "@/pages/setup/WizardRail";
import { canRewindTo, prevOf } from "@/pages/setup/steps";
import { useWizardNav } from "@/pages/setup/useWizardNav";
import { FooterElContext } from "@/pages/setup/wizardFooterContext";

/**
 * "Guided rail" wizard card — one stable shape across all steps.
 *
 * A fixed-height card split into three grid regions: the step rail
 * (left, spanning full height), a scrollable body, and a docked footer
 * that hosts each step's primary action via `WizardFooter`. Only the
 * body scrolls, so the footer button never moves between steps — the
 * core requirement of the redesign.
 *
 * The card collapses when its container is 600px or narrower: the rail
 * becomes a top progress bar, content stacks vertically, and a "Back"
 * link appears above the step content (on desktop, clicking rail rows
 * is the back affordance).
 */
export function WizardShell({
  current,
  children,
}: {
  current: WizardStep;
  children: ReactNode;
}) {
  const [footerEl, setFooterEl] = useState<HTMLDivElement | null>(null);
  const nav = useWizardNav(current);
  const prev = prevOf(current);
  const canGoBack = prev !== null && canRewindTo(current, prev);

  return (
    <div className="@container w-full max-w-[840px]">
      <div
        data-testid="setup-wizard-shell"
        className="grid h-[560px] grid-cols-1 grid-rows-[auto_1fr_auto] overflow-hidden rounded-xl border border-line bg-card shadow-card @min-[601px]:grid-cols-[236px_1fr] @min-[601px]:grid-rows-[1fr_auto]"
      >
        <div className="bg-card px-[30px] pt-6 pb-3.5 @min-[601px]:col-start-1 @min-[601px]:row-span-2 @min-[601px]:row-start-1 @min-[601px]:flex @min-[601px]:flex-col @min-[601px]:border-r @min-[601px]:border-line @min-[601px]:bg-sunken @min-[601px]:px-5 @min-[601px]:py-[26px]">
          <WizardRail current={current} />
        </div>

        <div className="overflow-y-auto px-[30px] pt-6 pb-7 @min-[601px]:col-start-2 @min-[601px]:row-start-1">
          {canGoBack && (
            <button
              type="button"
              onClick={nav.goBack}
              disabled={nav.isPending}
              data-testid="setup-back"
              className="mb-3 flex items-center gap-1.5 text-ink-3 transition-colors hover:text-ink @min-[601px]:hidden"
            >
              <ArrowLeftIcon className="size-3.5" />
              <Text size="sm" weight="medium">
                Back
              </Text>
            </button>
          )}
          <FooterElContext.Provider value={footerEl}>
            {children}
          </FooterElContext.Provider>
        </div>

        {/* WizardFooter (rendered by each step) portals its primary action
            into this element so the CTA is docked, outside the scroll flow. */}
        <div
          ref={setFooterEl}
          data-testid="setup-wizard-footer"
          className="border-t border-line bg-card px-[30px] py-4 @min-[601px]:col-start-2 @min-[601px]:row-start-2"
        />
      </div>
    </div>
  );
}
