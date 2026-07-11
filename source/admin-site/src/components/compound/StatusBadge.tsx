import type { ComponentProps, ReactNode } from "react";
import { CheckIcon } from "lucide-react";
import { Pill } from "@wardnet/web";

type StatusBadgeTone = "success" | "neutral" | "danger";

interface StatusBadgeProps extends Omit<
  ComponentProps<typeof Pill>,
  "variant" | "children"
> {
  /** Visual tone. `success` for desirable states (Running, Up to date, Active),
   *  `neutral` for off/idle states (Stopped, Disabled, External), `danger` for
   *  problem states (Reconnecting, Failed, Quarantined). */
  tone: StatusBadgeTone;
  /** Show a leading check icon. Per the design guidelines (§3.2), reserved for
   *  card-level success states that confirm a desirable system condition
   *  (Running, Up to date). Routine row-level statuses (Active, Lease,
   *  Enabled) should leave this off. */
  withIcon?: boolean;
  children: ReactNode;
}

const variantForTone = {
  success: "ok",
  neutral: "ghost",
  danger: "down",
} as const;

/** Status badge for a record's state. Wraps the {@link Pill} primitive with
 *  the tone vocabulary from §3.2 of `WEBUI-DESIGN-GUIDELINES.md` so call sites
 *  declare what the state *means* rather than picking a Forge variant. */
export function StatusBadge({
  tone,
  withIcon = false,
  children,
  ...rest
}: StatusBadgeProps) {
  return (
    // eslint-disable-next-line security/detect-object-injection -- tone is a closed-union prop (success/neutral/danger) into a local const map
    <Pill variant={variantForTone[tone]} {...rest}>
      {withIcon && tone === "success" && <CheckIcon />}
      {children}
    </Pill>
  );
}
