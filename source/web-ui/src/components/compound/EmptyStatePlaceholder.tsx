import { type ReactNode } from "react";
import { FileText, PlusCircle } from "lucide-react";
import { Button } from "@/components/core/ui/button";

interface EmptyStatePlaceholderProps {
  /** Primary message. */
  message: string;
  /** Secondary hint text. */
  hint?: string;
  /** Action button label. */
  actionLabel?: string;
  /** Action button callback. */
  onAction?: () => void;
  /** Override the default icon. */
  icon?: ReactNode;
}

/**
 * Empty-state placeholder for user-created data (reservations, tunnels, etc.).
 *
 * Centered layout with a document icon, concentric ripple rings, a message,
 * and an optional action button. Uses the same muted primary color scheme
 * as the DiscoveryPlaceholder.
 */
export function EmptyStatePlaceholder({
  message,
  hint,
  actionLabel,
  onAction,
  icon,
}: EmptyStatePlaceholderProps) {
  return (
    <div className="flex min-h-64 flex-1 flex-col items-center justify-center rounded-xl border border-border bg-background">
      <style>{`
        @keyframes ripple-1 { 0%, 100% { opacity: 0.08; transform: scale(1); } 50% { opacity: 0.15; transform: scale(1.05); } }
        @keyframes ripple-2 { 0%, 100% { opacity: 0.05; transform: scale(1); } 50% { opacity: 0.1; transform: scale(1.03); } }
        @keyframes ripple-3 { 0%, 100% { opacity: 0.03; transform: scale(1); } 50% { opacity: 0.06; transform: scale(1.02); } }
      `}</style>

      {/* Concentric ripple rings + icon */}
      <div className="relative flex items-center justify-center">
        {/* Outer ring */}
        <div
          className="absolute size-56 rounded-full border border-primary/10 bg-primary/[0.02]"
          style={{ animation: "ripple-3 4s ease-in-out infinite" }}
        />
        {/* Middle ring */}
        <div
          className="absolute size-40 rounded-full border border-primary/15 bg-primary/[0.03]"
          style={{ animation: "ripple-2 4s ease-in-out 0.3s infinite" }}
        />
        {/* Inner ring */}
        <div
          className="absolute size-28 rounded-full border border-primary/20 bg-primary/[0.04]"
          style={{ animation: "ripple-1 4s ease-in-out 0.6s infinite" }}
        />

        {/* Icon group */}
        <div className="relative z-10 flex items-end">
          {icon ?? (
            <>
              {/* Stacked document effect */}
              <div className="relative">
                <FileText className="size-16 text-primary/20" strokeWidth={1.2} />
                <FileText
                  className="absolute -left-1.5 -top-1.5 size-16 text-primary/10"
                  strokeWidth={1}
                />
              </div>
              {/* Plus badge */}
              <PlusCircle className="-ml-4 mb-0.5 size-8 text-primary/30" strokeWidth={1.5} />
            </>
          )}
        </div>
      </div>

      {/* Text + action */}
      <div className="mt-8 flex flex-col items-center gap-3 px-4 text-center">
        <p className="text-sm font-medium text-muted-foreground">{message}</p>
        {hint && <p className="max-w-sm text-xs text-muted-foreground/60">{hint}</p>}
        {actionLabel && onAction && (
          <Button onClick={onAction} className="mt-2">
            {actionLabel}
          </Button>
        )}
      </div>
    </div>
  );
}
