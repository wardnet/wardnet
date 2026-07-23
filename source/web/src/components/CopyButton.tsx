import { useEffect, useRef, useState } from "react";
import { CheckIcon, CopyIcon } from "lucide-react";
import { Button, toast } from "@wardnet/ui";

interface CopyButtonProps {
  /** The text written to the clipboard on click. */
  value: string;
  /** Button label shown next to the icon (default "Copy"). */
  label?: string;
  /** Label shown briefly after a successful copy (default "Copied"). */
  copiedLabel?: string;
  className?: string;
  "data-testid"?: string;
}

/**
 * Copy-to-clipboard button — the repo's first clipboard affordance, shared so
 * admin-site and the user PWA copy identically (the Android Private DNS
 * hostname has to be pasted by hand into Settings, so a reliable copy button is
 * the whole UX). Flips to a "Copied" confirmation for a moment, then resets.
 */
export function CopyButton({
  value,
  label = "Copy",
  copiedLabel = "Copied",
  className,
  "data-testid": testId,
}: CopyButtonProps) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Clear the pending reset on unmount so we never setState after teardown.
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1500);
    } catch {
      toast.error("Couldn't copy to the clipboard");
    }
  }

  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      onClick={handleCopy}
      className={className}
      data-testid={testId}
      aria-label={copied ? copiedLabel : label}
    >
      {copied ? (
        <CheckIcon size={16} aria-hidden />
      ) : (
        <CopyIcon size={16} aria-hidden />
      )}
      {copied ? copiedLabel : label}
    </Button>
  );
}
