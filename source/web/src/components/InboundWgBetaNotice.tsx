import { SparklesIcon } from "lucide-react";
import { Text } from "@wardnet/ui";

interface Props {
  /** Extra classes for spacing in the host layout (margins, width). */
  className?: string;
}

/**
 * Shared notice shown on both inbound-WireGuard ("Personal VPN") enablement
 * surfaces (the admin-site VPN page and the admin-app grant sheet) while the
 * feature ships as a beta: remote connectivity is not fully wired yet, and the
 * feature is Premium-only. Kept in one place so the copy stays identical across
 * both apps.
 */
export function InboundWgBetaNotice({ className }: Props) {
  return (
    <div
      className={[
        "flex items-start gap-2.5 rounded-lg border border-line bg-sunken px-3.5 py-3",
        className ?? "",
      ].join(" ")}
      data-testid="inbound-wg-beta-notice"
    >
      <SparklesIcon size={16} className="mt-0.5 shrink-0 text-accent" />
      <Text as="p" size="xs" className="text-ink-3">
        <Text as="span" size="xs" weight="medium" className="text-ink">
          Personal VPN is in beta.
        </Text>{" "}
        You can set it up now, but connecting to it from outside your network is
        rolling out soon. It requires a Premium subscription.
      </Text>
    </div>
  );
}
