import * as React from "react";
import { clsx } from "clsx";
import s from "./banner.module.css";

type BannerTone = "down" | "warn" | "info";

const toneClass: Record<BannerTone, string> = {
  down: s.down,
  warn: s.warn,
  info: s.info,
};

type BannerProps = Omit<React.ComponentProps<"div">, "children" | "role"> & {
  /**
   * Tone variant — drives the soft-bg + soft-ink + bottom-border colors via
   * `.banner--down|--warn|--info`. Required (no default) so consumers make
   * the operator-signal classification explicit at the call site.
   */
  tone: BannerTone;
  /**
   * Optional leading icon rendered in the `.banner__icon` slot. Pass a
   * lucide-react element (or any 16px SVG); CSS sizes it to 16x16 so
   * call sites don't need `size-4` / `width="16"`.
   */
  icon?: React.ReactNode;
  /** Banner message — rendered in the `.banner__msg` slot. */
  children: React.ReactNode;
  /**
   * Optional right-aligned slot for buttons (Dismiss, Retry, …). Rendered
   * in `.banner__actions` only when provided so the flex layout collapses
   * cleanly on icon + message variants.
   */
  actions?: React.ReactNode;
  /**
   * ARIA live-region semantics. `"alert"` is for assertive interruptions
   * (daemon down, unclean shutdown); `"status"` is for advisory notices
   * (info-tone updates). Defaults to `"status"` — promote to `"alert"`
   * only when the message warrants assistive-tech interruption.
   */
  role?: "alert" | "status";
};

// Why a slot-based, tone-required compound: the three legacy banners
// (ConnectionBanner, UncleanShutdownBanner, future info banners) all share
// the same shape — full-width strip, leading icon, message, optional actions.
// One primitive + three tones beats three near-duplicate components and keeps
// the soft-bg / soft-ink token pairing aligned with `.pill--*` and the
// toaster bridge (see styles.css §05 banner block).
function Banner({
  className,
  tone,
  icon,
  children,
  actions,
  role = "status",
  ...props
}: BannerProps) {
  return (
    <div
      data-slot="banner"
      role={role}
      className={clsx(s.banner, toneClass[tone], className)}
      {...props}
    >
      {icon !== undefined && <span className={s.icon}>{icon}</span>}
      <span className={s.msg}>{children}</span>
      {actions !== undefined && <span className={s.actions}>{actions}</span>}
    </div>
  );
}

export { Banner };
export type { BannerProps, BannerTone };
