import { CircleAlertIcon, X } from "lucide-react";
import { Banner } from "@wardnet/web";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { useAcknowledgeShutdown, useSystemStatus } from "@wardnet/web";
import { timeAgo } from "@wardnet/web";

/**
 * Full-width banner that calls out the previous unclean daemon
 * shutdown.
 *
 * Visible iff the most recent shutdown classification is `unclean`
 * AND there is no acknowledgement timestamp at-or-after the event
 * timestamp. Dismissing the banner posts to
 * `/api/system/shutdown/acknowledge`, which records `acknowledged_at`.
 * A future unclean event automatically resurfaces the banner because
 * the new event timestamp is newer than the stored ack — no explicit
 * "reset on event" coupling is required.
 *
 * Thin data-coupled wrapper over the Forge `<Banner>` primitive — visual
 * shape (full-width danger-soft strip) lives in the `.banner` recipe
 * (styles.css §05); this component owns the visibility predicate and the
 * Dismiss action.
 */
export function UncleanShutdownBanner() {
  const { data: status } = useSystemStatus();
  const ack = useAcknowledgeShutdown();

  if (!status) return null;

  const { last_shutdown: shutdown } = status;
  if (shutdown.state !== "unclean" || !shutdown.at) return null;

  // Banner predicate: hide once the ack is at-or-after the event.
  if (shutdown.acknowledged_at && shutdown.acknowledged_at >= shutdown.at) {
    return null;
  }

  return (
    <Banner
      tone="down"
      role="alert"
      icon={<CircleAlertIcon />}
      actions={
        <Button
          size="sm"
          variant="ghost"
          onClick={() => ack.mutate()}
          disabled={ack.isPending}
          aria-label="Dismiss unclean shutdown banner"
        >
          <X className="mr-1 size-3.5" />
          Dismiss
        </Button>
      }
    >
      <Text as="span" weight="medium">
        Wardnet did not shut down cleanly
      </Text>
      <span className="ml-2 opacity-80">
        Last seen {timeAgo(shutdown.at)} — likely a crash or power loss.
      </span>
    </Banner>
  );
}
