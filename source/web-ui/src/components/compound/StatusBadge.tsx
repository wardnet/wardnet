import type { ReactNode } from "react";
import { CheckIcon } from "lucide-react";
import { Badge } from "@/components/core/ui/badge";

type StatusBadgeTone = "success" | "neutral";

interface StatusBadgeProps {
  /** Visual tone. `success` for desirable states (Running, Up to date, Active),
   *  `neutral` for off/idle states (Stopped, Disabled, External). */
  tone: StatusBadgeTone;
  /** Show a leading check icon. Per the design guidelines (§3.2), reserved for
   *  card-level success states that confirm a desirable system condition
   *  (Running, Up to date). Routine row-level statuses (Active, Lease,
   *  Enabled) should leave this off. */
  withIcon?: boolean;
  children: ReactNode;
}

const variantForTone = {
  success: "success",
  neutral: "secondary",
} as const;

/** Status badge for a record's state. Wraps the {@link Badge} primitive with
 *  the tone vocabulary from §3.2 of `WEBUI-DESIGN-GUIDELINES.md` so call sites
 *  declare what the state *means* rather than picking a Tailwind variant. */
export function StatusBadge({ tone, withIcon = false, children }: StatusBadgeProps) {
  return (
    <Badge variant={variantForTone[tone]}>
      {withIcon && tone === "success" && <CheckIcon />}
      {children}
    </Badge>
  );
}
