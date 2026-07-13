import { TriangleAlertIcon } from "lucide-react";
import { Text } from "@wardnet/ui";
import { useDnsFilterConfig } from "../hooks/useDnsFilter";

interface Props {
  /** Extra classes for spacing in the host layout (margins, width). */
  className?: string;
}

/**
 * Warns that DNS filtering is enabled but **not currently being enforced**.
 *
 * The daemon answers DNS as soon as its server binds, while the compiled filters
 * are built in a background task — and a blocklist that has never been
 * downloaded is an empty list, so it is "enabled" while blocking nothing. That
 * happens on every restart (briefly) and for several minutes after the upgrade
 * that reset the blocklist cache, during which malware and adult-content lists
 * are silently inert.
 *
 * Failing open is the right call — a gateway that answers no DNS until its lists
 * finish downloading takes the whole LAN offline — but it must not be silent,
 * which is what this notice is for. It disappears on its own once every enabled
 * list has been imported.
 */
export function DnsFilterNotReadyNotice({ className }: Props) {
  const { data } = useDnsFilterConfig();

  // No data yet, or filtering is genuinely in force — say nothing.
  if (!data || data.filters_ready) return null;

  // The global kill switch is off, so nothing is *meant* to be enforced. A
  // warning here would be noise about a state the admin chose.
  if (!data.config.enabled) return null;

  const pending = data.pending_blocklists;

  return (
    <div
      className={[
        "flex items-start gap-2.5 rounded-lg border border-line bg-sunken px-3.5 py-3",
        className ?? "",
      ].join(" ")}
      role="status"
      data-testid="dns-filter-not-ready-notice"
    >
      <TriangleAlertIcon size={16} className="mt-0.5 shrink-0 text-warning" />
      <Text as="p" size="xs" className="text-ink-3">
        <Text as="span" size="xs" weight="medium" className="text-ink">
          DNS filtering is not being enforced yet.
        </Text>{" "}
        {pending > 0
          ? `${pending} enabled ${pending === 1 ? "blocklist is" : "blocklists are"} still downloading. Until that finishes they are empty, so nothing they cover is blocked.`
          : "The filters are still loading. Queries are being answered unfiltered until they are ready."}{" "}
        Browsing keeps working throughout — this clears on its own.
      </Text>
    </div>
  );
}
