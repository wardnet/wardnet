import { useRef } from "react";
import { RefreshCw } from "lucide-react";
import { Button, Heading, Text } from "@wardnet/ui";
import { useDaemonStatus } from "../hooks/useDaemonStatus";

interface ConnectionGateProps {
  children: React.ReactNode;
  /**
   * Whether this app surface is Premium-gated. Defaults to `true` (the two
   * mobile PWAs, user-app and admin-app). The desktop admin website
   * (admin-site) must always stay reachable regardless of entitlement — the
   * daemon's serving layer already exempts it (see `Surface::AdminSite` in
   * `web.rs`) — so admin-site passes `premiumGated={false}` here to match.
   */
  premiumGated?: boolean;
}

/**
 * Blocks the app behind a full-screen "can't reach the daemon" page until the
 * very first successful `/api/info` probe, and, for Premium-gated surfaces,
 * behind a full-screen "premium required" page for as long as the box isn't
 * entitled to this app.
 *
 * The *reachability* gate only blocks on a *cold* start (we've never reached
 * the daemon). Once a healthy connection has been seen, transient drops are
 * handled by the non-blocking `ConnectionBanner` instead — a live session
 * (e.g. mid-restart, which has its own dialog flow) must not be yanked to a
 * full-screen error.
 *
 * The *entitlement* gate is deliberately **not** latched the same way: unlike
 * a transient reachability blip, losing entitlement (e.g. a subscription
 * lapsing) must re-block an already-open session, not just a cold start. This
 * is also what makes an already-installed PWA self-gate — its shell is served
 * from the service-worker precache and never re-hits the server-side serving
 * gate, so this client-side check (on the same poll as reachability) is the
 * only thing that catches it.
 *
 * Mount this around the app root *and* any always-on stream managers so they
 * don't open sockets (and reconnect-storm) while the daemon is unreachable.
 * Recovery is automatic: `useDaemonStatus` polls every 5 s while down / 30 s
 * once healthy, so both gates lift within that same window.
 */
export function ConnectionGate({
  children,
  premiumGated = true,
}: ConnectionGateProps) {
  const { data, isLoading, isFetching, refetch } = useDaemonStatus();
  const hasConnected = useRef(false);

  // Latch ref: once we've seen a reachable daemon, stay unblocked forever.
  // Writing and reading the ref in render is deliberate (the latch flips on
  // the same render the query resolves), so opt these two lines out of the
  // render-time ref rule.
  // eslint-disable-next-line react-hooks/refs
  if (data?.reachable) hasConnected.current = true;

  // eslint-disable-next-line react-hooks/refs
  if (!hasConnected.current) {
    // First probe still in flight — show a quiet splash, not a flash of error.
    if (isLoading || !data) return <ConnectionSplash />;

    // `data.reachable` must be `false` here: line 55 above would have latched
    // `hasConnected.current` to `true` (skipping this whole block) on any
    // render where it was `true`. Unconditional rather than a redundant
    // `if (!data.reachable)` — the alternative is dead code no test can hit.
    return (
      <ConnectionError onRetry={() => void refetch()} retrying={isFetching} />
    );
  }

  // Checked on every poll, not latched — `entitled === false` is an explicit
  // denial (as opposed to `null`, which means "unknown, daemon unreachable").
  if (premiumGated && data?.entitled === false) {
    return <PremiumRequired />;
  }

  return <>{children}</>;
}

/** Minimal centered spinner shown while the first reachability probe resolves. */
function ConnectionSplash() {
  return (
    <div className="empty col items-center justify-center">
      <RefreshCw
        size={22}
        strokeWidth={2}
        className="animate-spin"
        aria-label="Connecting to Wardnet"
      />
    </div>
  );
}

interface FullScreenNoticeProps {
  heading: React.ReactNode;
  body: React.ReactNode;
  action?: React.ReactNode;
}

/**
 * Shared layout for a full-screen, whole-page interstitial state (mirrors the
 * 404 page's Forge `.empty` block). `ConnectionError` and `PremiumRequired`
 * both render this so a layout tweak (spacing, heading size, wrapper class)
 * only needs to be made once.
 */
function FullScreenNotice({ heading, body, action }: FullScreenNoticeProps) {
  return (
    <div role="alert" className="empty col items-center justify-center gap-4">
      <Heading level={2} size="3xl" className="text-ink">
        {heading}
      </Heading>
      <Text as="p" size="sm" className="mt-1 max-w-md text-ink-3">
        {body}
      </Text>
      {action}
    </div>
  );
}

interface ConnectionErrorProps {
  onRetry: () => void;
  retrying: boolean;
}

/** Full-screen interstitial shown when the daemon can't be reached on load. */
function ConnectionError({ onRetry, retrying }: ConnectionErrorProps) {
  return (
    <FullScreenNotice
      heading="Can’t reach Wardnet"
      body="The Wardnet daemon isn’t responding. Make sure it’s running and that you’re on the same network, then try again."
      action={
        <Button variant="outline" onClick={onRetry} disabled={retrying}>
          <RefreshCw
            size={15}
            strokeWidth={2}
            className={retrying ? "animate-spin" : undefined}
          />
          {retrying ? "Retrying…" : "Retry"}
        </Button>
      }
    />
  );
}

/**
 * Full-screen interstitial shown when the box isn't entitled to this app —
 * either it never subscribed (free/BYO-domain) or a subscription lapsed.
 * There's no in-app retry action: entitlement changes on the daemon side (a
 * fresh subscribe/resubscribe), and the gate lifts on its own within one poll
 * of that happening.
 *
 * Keep this copy in sync with `premium_required_response()`'s
 * `PREMIUM_REQUIRED_HTML` in `source/daemon/crates/wardnetd-api/src/web.rs` —
 * that's the server-side equivalent of this same "premium required" state.
 */
function PremiumRequired() {
  return (
    <FullScreenNotice
      heading="This app requires wardnet Premium"
      body="The mobile apps are a Premium capability. Subscribe or check your subscription status from the admin website. Everything on your network keeps running locally — only these apps are paused."
    />
  );
}
