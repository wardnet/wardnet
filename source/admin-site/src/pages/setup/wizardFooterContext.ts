import { createContext, useContext } from "react";

/**
 * Portal target for `WizardFooter`: the enclosing `WizardShell`'s docked
 * footer element, or `null` when a step renders outside the shell
 * (component tests).
 */
export const FooterElContext = createContext<HTMLDivElement | null>(null);

/** The docked footer element of the enclosing `WizardShell`, if any. */
export function useWizardFooterEl(): HTMLDivElement | null {
  return useContext(FooterElContext);
}
