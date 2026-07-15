import { useState } from "react";
import { DownloadIcon } from "lucide-react";
import { useLocation } from "react-router";
import { Text, toast } from "@wardnet/ui";

import { useInstallPrompt } from "../hooks/useInstallPrompt";

interface InstallPromptProps {
  /** The app's own icon asset, shown at the left of the banner. */
  icon: string;
}

/**
 * Home-screen install banner shared by the user and admin PWAs, so both apps
 * promote installation with identical placement and behaviour.
 *
 * Renders in flow at the top of the layout (place it between the connection
 * banner and `<main>`): a card overlaid on the bottom of a `100vh` container
 * gets clipped behind mobile browser chrome — which is exactly where the
 * banner matters, since an installed app never shows it.
 *
 * Shows only on the app's home route, and only while the browser reports the
 * app as installable; dismissing it hides it for the session.
 */
export function InstallPrompt({ icon }: InstallPromptProps) {
  const { isInstallable, promptInstall } = useInstallPrompt();
  const location = useLocation();
  const [dismissed, setDismissed] = useState(false);

  if (!isInstallable || dismissed || location.pathname !== "/") return null;

  async function handleInstall() {
    try {
      const result = await promptInstall();
      if (result?.outcome === "accepted") {
        toast.success("Added to home screen");
      }
    } catch {
      // Browser cancelled or the prompt is no longer available — dismiss silently.
    } finally {
      setDismissed(true);
    }
  }

  return (
    <div className="mx-3 mt-3 flex animate-slide-down items-center gap-3 rounded-lg border border-line bg-card p-3.5 shadow-pop">
      <img src={icon} alt="" className="h-9 w-9 shrink-0 rounded-[10px]" />
      <div className="min-w-0 flex-1">
        <Text
          as="p"
          size="base"
          weight="semibold"
          className="leading-tight text-ink"
        >
          Install Wardnet
        </Text>
        <Text as="p" size="xs" className="mt-0.5 text-ink-3">
          Add to home screen for one-tap access
        </Text>
      </div>
      <button
        onClick={() => setDismissed(true)}
        className="rounded-md border border-line-strong px-3 py-1.5 text-[13px] font-medium text-ink"
      >
        Later
      </button>
      <button
        onClick={handleInstall}
        className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-[13px] font-medium text-accent-ink"
      >
        <DownloadIcon size={14} strokeWidth={2} />
        Install
      </button>
    </div>
  );
}
