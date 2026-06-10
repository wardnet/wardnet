import { type ReactNode } from "react";
import { brand } from "@wardnet/styles/tokens";
import { FileText, PlusCircle } from "lucide-react";
import { Button } from "@wardnet/web";

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
 * Outer frame consumes the Forge `.empty` class (dashed border, centred
 * `--ink-3` text, `--radius-lg`). The unique concentric ripple rings +
 * stacked-document icon + optional action button sit inside that frame and
 * use Forge tokens (`--accent`, `--ink-3`) directly via `color-mix` so the
 * tinted ring/icon hierarchy survives without any raw Tailwind palette refs.
 */
export function EmptyStatePlaceholder({
  message,
  hint,
  actionLabel,
  onAction,
  icon,
}: EmptyStatePlaceholderProps) {
  // Pre-computed token tints keep the rings + icon hierarchy that defines
  // this component's identity, while staying inside the Forge palette.
  const ringOuterBorder = `color-mix(in oklab, ${brand.accent} 10%, transparent)`;
  const ringOuterBg = `color-mix(in oklab, ${brand.accent} 2%, transparent)`;
  const ringMidBorder = `color-mix(in oklab, ${brand.accent} 15%, transparent)`;
  const ringMidBg = `color-mix(in oklab, ${brand.accent} 3%, transparent)`;
  const ringInnerBorder = `color-mix(in oklab, ${brand.accent} 20%, transparent)`;
  const ringInnerBg = `color-mix(in oklab, ${brand.accent} 4%, transparent)`;
  const docFront = `color-mix(in oklab, ${brand.accent} 20%, transparent)`;
  const docBack = `color-mix(in oklab, ${brand.accent} 10%, transparent)`;
  const plusBadge = `color-mix(in oklab, ${brand.accent} 30%, transparent)`;
  const hintColor = "color-mix(in oklab, var(--ink-3) 60%, transparent)";

  return (
    <div className="empty flex min-h-64 flex-1 flex-col items-center justify-center">
      <style>{`
        @keyframes ripple-1 { 0%, 100% { opacity: 0.08; transform: scale(1); } 50% { opacity: 0.15; transform: scale(1.05); } }
        @keyframes ripple-2 { 0%, 100% { opacity: 0.05; transform: scale(1); } 50% { opacity: 0.1; transform: scale(1.03); } }
        @keyframes ripple-3 { 0%, 100% { opacity: 0.03; transform: scale(1); } 50% { opacity: 0.06; transform: scale(1.02); } }
      `}</style>

      {/* Concentric ripple rings + icon */}
      <div className="relative flex items-center justify-center">
        {/* Outer ring */}
        <div
          className="absolute size-56 rounded-full border"
          style={{
            borderColor: ringOuterBorder,
            background: ringOuterBg,
            animation: "ripple-3 4s ease-in-out infinite",
          }}
        />
        {/* Middle ring */}
        <div
          className="absolute size-40 rounded-full border"
          style={{
            borderColor: ringMidBorder,
            background: ringMidBg,
            animation: "ripple-2 4s ease-in-out 0.3s infinite",
          }}
        />
        {/* Inner ring */}
        <div
          className="absolute size-28 rounded-full border"
          style={{
            borderColor: ringInnerBorder,
            background: ringInnerBg,
            animation: "ripple-1 4s ease-in-out 0.6s infinite",
          }}
        />

        {/* Icon group */}
        <div className="relative z-10 flex items-end">
          {icon ?? (
            <>
              {/* Stacked document effect */}
              <div className="relative">
                <FileText
                  className="size-16"
                  strokeWidth={1.2}
                  style={{ color: docFront }}
                />
                <FileText
                  className="absolute -left-1.5 -top-1.5 size-16"
                  strokeWidth={1}
                  style={{ color: docBack }}
                />
              </div>
              {/* Plus badge */}
              <PlusCircle
                className="-ml-4 mb-0.5 size-8"
                strokeWidth={1.5}
                style={{ color: plusBadge }}
              />
            </>
          )}
        </div>
      </div>

      {/* Text + action */}
      <div className="mt-8 flex flex-col items-center gap-3 px-4 text-center">
        <p className="text-sm font-medium" style={{ color: "var(--ink-3)" }}>
          {message}
        </p>
        {hint && (
          <p className="max-w-sm text-xs" style={{ color: hintColor }}>
            {hint}
          </p>
        )}
        {actionLabel && onAction && (
          <Button onClick={onAction} className="mt-2">
            {actionLabel}
          </Button>
        )}
      </div>
    </div>
  );
}
