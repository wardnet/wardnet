import { useState } from "react";
import { ShieldIcon, DownloadIcon } from "lucide-react";
import { useInstallPrompt } from "@wardnet/web";
import { useLocation } from "react-router";
import { toast } from "sonner";

export function InstallPrompt() {
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
    <div className="absolute inset-x-3 bottom-3 z-30 flex animate-slide-up items-center gap-3 rounded-lg border border-line bg-card p-3.5 shadow-pop">
      <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-accent text-accent-ink">
        <ShieldIcon size={20} strokeWidth={2} />
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-[14px] font-semibold leading-tight text-ink">Install Wardnet</p>
        <p className="mt-0.5 text-[12px] text-ink-3">Add to home screen for one-tap access</p>
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
