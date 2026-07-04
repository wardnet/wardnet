import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import { useWizardFooterEl } from "@/pages/setup/wizardFooterContext";

/**
 * Docks a step's primary action into the wizard shell's footer region.
 *
 * Steps render `<WizardFooter>…</WizardFooter>` in their normal tree;
 * the content portals into the shell's fixed footer so it sits outside
 * the scrollable body and never moves between steps. When no shell is
 * mounted (step component tests render steps standalone) the children
 * render inline instead, keeping existing tests harness-free.
 */
export function WizardFooter({ children }: { children: ReactNode }) {
  const footerEl = useWizardFooterEl();
  if (!footerEl) return <div className="mt-5">{children}</div>;
  return createPortal(children, footerEl);
}
