import { CircleAlertIcon, X } from "lucide-react";
import { Button } from "@wardnet/forge-web/button";
import { useAcknowledgeShutdown, useSystemStatus } from "@/hooks/useSystemStatus";
import { timeAgo } from "@/lib/utils";

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
 * Styling mirrors the destructive tone of `ApiErrorAlert` but renders
 * full-width above the page content (à la `ConnectionBanner`) so the
 * operator notices it on every authenticated route until they
 * acknowledge it.
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
    <div
      role="alert"
      className="flex items-center gap-3 border-b border-danger/30 bg-danger/10 px-4 py-2.5 text-sm text-danger"
    >
      <CircleAlertIcon className="size-4 shrink-0" />
      <div className="flex-1">
        <span className="font-medium">Wardnet did not shut down cleanly</span>
        <span className="ml-2 text-danger/80">
          Last seen {timeAgo(shutdown.at)} — likely a crash or power loss.
        </span>
      </div>
      <Button
        size="sm"
        variant="ghost"
        className="h-7 px-2 text-danger hover:bg-danger/20 hover:text-danger"
        onClick={() => ack.mutate()}
        disabled={ack.isPending}
        aria-label="Dismiss unclean shutdown banner"
      >
        <X className="mr-1 size-3.5" />
        Dismiss
      </Button>
    </div>
  );
}
