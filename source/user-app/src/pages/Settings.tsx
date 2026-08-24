import { WifiOffIcon } from "lucide-react";
import {
  ApiErrorAlert,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  PrivateDnsInstructions,
  AccessRequestStatusPill,
  Text,
  Toggle,
  isIosBrowserTab,
  privateDnsService,
  useMyDevice,
  useCreateAccessRequest,
  useMyAccessRequests,
  usePrivateDnsMe,
  usePushNotifications,
  useSetMyCaptureEnabled,
} from "@wardnet/web";
import { useHashScrollTarget } from "@/hooks/useHashScrollTarget";

/**
 * Notifications card (issue #594): enable/disable Web Push for this browser.
 * Subscriptions are device-keyed on the daemon — the device gets notified
 * when an admin locks/unlocks or changes its routing.
 */
function Notifications() {
  const push = usePushNotifications();

  const helperText =
    push.state === "denied"
      ? "Notifications are blocked in your browser settings."
      : push.state === "unsupported"
        ? isIosBrowserTab()
          ? "Install the app to your Home Screen to enable notifications."
          : "Notifications are not supported in this browser."
        : "Get notified when your administrator locks or changes your " +
          "device's routing, even when the app is closed.";

  return (
    <Card>
      <CardHeader>
        <CardTitle>Notifications</CardTitle>
        <span className="ml-auto">
          <Toggle
            checked={push.state === "subscribed"}
            onCheckedChange={(checked) =>
              checked ? void push.subscribe() : void push.unsubscribe()
            }
            disabled={
              push.state === "unsupported" ||
              push.state === "denied" ||
              push.isBusy
            }
            aria-label="Enable push notifications"
            data-testid="notifications-toggle"
          />
        </span>
      </CardHeader>
      <CardContent>
        <Text as="p" size="sm" className="text-ink-3">
          {helperText}
        </Text>
      </CardContent>
    </Card>
  );
}

function MyRequests() {
  const { data, isLoading } = useMyAccessRequests();

  // `private_dns` requests are deliberately excluded: the Private DNS card
  // below owns that state end to end (pending → declined → setup steps), and
  // listing them here too would show the member the same request twice, in two
  // places, saying different things.
  const requests = (data ?? []).filter((r) => r.kind !== "private_dns");

  if (isLoading || requests.length === 0) {
    return null;
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>My requests</CardTitle>
      </CardHeader>
      <CardContent>
        <ul className="flex flex-col gap-2" data-testid="my-requests">
          {requests.map((r) => (
            <li key={r.id} className="flex items-center justify-between gap-3">
              <span className="min-w-0">
                <Text
                  as="span"
                  size="sm"
                  className="truncate font-mono text-ink"
                >
                  {r.domain}
                </Text>
                <Text as="span" size="xs" className="block text-ink-3">
                  {r.kind === "block" ? "Block request" : "Allow request"}
                </Text>
              </span>
              <AccessRequestStatusPill status={r.status} />
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}

/** Fragment the `private_dns_granted` push appends to `/settings`. */
const PRIVATE_DNS_HASH = "#private-dns";

/**
 * Private DNS card (issues #910/#916): encrypted DNS setup for *this* device.
 *
 * Device-keyed like the rest of the PWA — the daemon resolves the caller by the
 * source IP of the TCP connection, so this only ever describes the phone that
 * is reading it. That is also why there is no QR: on the admin site the reader
 * is on a different screen and needs one, here the phone would be scanning
 * itself. Hence `variant="on-device"`.
 *
 * The member can *ask* for a grant from here (#919), but only once the feature
 * is enabled network-wide: `grant_device` requires it, so a request raised
 * while Private DNS is off would be one the admin cannot approve. The
 * not-enabled state therefore stays informational — that prerequisite is the
 * admin's to fix, and the daemon refuses such a request anyway.
 */
function PrivateDns() {
  const { data: me, isLoading: grantLoading } = usePrivateDnsMe();
  const { data: requests, isLoading: requestsLoading } = useMyAccessRequests();
  const createRequest = useCreateAccessRequest();

  // Both queries gate the card, because the ungranted branch is written from
  // *both*. Reading only the grant's `isLoading` would, on a cold open, show a
  // member who already asked "This device hasn't been granted Private DNS yet"
  // with a live Request button — and tapping it earns a 409 from the partial
  // unique index, surfaced as an error for doing nothing wrong.
  const isLoading = grantLoading || requestsLoading;

  // This device's most recent Private-DNS request. The list is newest-first,
  // and the partial unique index means at most one can be open at a time.
  //
  // Read from the access-requests query rather than from `/private-dns/me`
  // deliberately: extending that response would make `PrivateDnsService` depend
  // on `AccessRequestService`, while approving a request already depends the
  // other way — a cycle. One extra query is the cost of keeping that graph
  // one-way.
  const request = (requests ?? []).find((r) => r.kind === "private_dns");

  // The push deep-links to `/settings#private-dns`, but this card is the last
  // of four — without this the member taps "Private DNS is ready" and lands on
  // the DNS-capture toggle with the setup steps off-screen.
  //
  // The three cards above are all data-driven and expand as their queries
  // resolve, so a scroll fired once at mount lands on the card's "Loading…"
  // line and is then pushed back below the fold by its own siblings (#1176).
  // `useHashScrollTarget` holds the position while the page settles instead;
  // `isLoading` re-arms it for a query that resolves after that window. The
  // card shell renders in every state, so the ref is always attached.
  const cardRef = useHashScrollTarget<HTMLDivElement>(
    PRIVATE_DNS_HASH,
    isLoading,
  );

  // Each branch names a distinct server state. In particular a granted device
  // can still come back with no hostname: `/private-dns/me` resolves the domain
  // lazily and degrades to `hostname: null` on a DDNS hiccup rather than
  // failing the call, so "granted" and "has a hostname" are not the same
  // question. Collapsing them would tell a granted member to go ask for a grant
  // they already hold — advice their admin can't act on.
  // Deliberately keyed on `!me` rather than `isError`: React Query keeps the
  // last successful data when a *refetch* fails, and `usePrivateDnsMe` now
  // refetches on window focus. Off-LAN the daemon is unreachable, so every
  // foreground would otherwise replace working setup steps with an error —
  // for a feature whose whole point is roaming. A stale hostname is still the
  // right hostname, so show it and only surface the error when we have nothing.
  const message = isLoading
    ? "Loading…"
    : !me
      ? "Couldn't check this device's Private DNS status. Pull to refresh, or try again in a moment."
      : !me.enabled
        ? "Private DNS isn't enabled on your network yet. Ask your administrator to turn it on."
        : !me.granted
          ? // Ungranted splits three ways once the member can ask. The
            // approved case is deliberately absent: an approved request means
            // a grant exists, so `me.granted` is true and we never land here.
            request?.status === "pending"
            ? "Requested — waiting for your administrator."
            : request?.status === "rejected"
              ? "Your request was declined. You can ask again if something's changed."
              : "This device hasn't been granted Private DNS yet."
          : !me.hostname
            ? // Granted, but the domain lookup didn't resolve — transient and
              // self-healing, so don't send the member to their admin over it.
              "This device is set up for Private DNS, but its hostname isn't available right now. Check back in a moment."
            : null;

  // Non-null exactly when every check above passed, so the setup view can rely
  // on it without an assertion.
  const hostname = message === null ? (me?.hostname ?? null) : null;

  // Offer the ask only where the admin can actually act on it: the feature is
  // on, this device has no grant, and nothing is already waiting. A declined
  // request re-opens the button rather than closing the door — the household
  // circumstance that prompted the "no" may have changed.
  //
  // `approved` is excluded as well as `pending`. The two queries resolve
  // independently and both refetch on focus, so there is a real window where
  // the access-requests query has landed on `approved` while `usePrivateDnsMe`
  // still holds a stale `granted: false` — offering the button there earns a
  // 409 from the already-granted guard for doing nothing wrong.
  const canRequest =
    !isLoading &&
    !!me?.enabled &&
    !me.granted &&
    request?.status !== "pending" &&
    request?.status !== "approved";

  return (
    <Card
      id="private-dns"
      ref={cardRef}
      className="scroll-mt-4"
      data-testid="private-dns-card"
    >
      <CardHeader>
        <CardTitle>Private DNS</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {hostname === null ? (
          <>
            <Text as="p" size="sm" className="text-ink-3">
              {message}
            </Text>
            {canRequest && (
              <Button
                onClick={() => createRequest.mutate({ kind: "private_dns" })}
                disabled={createRequest.isPending}
                data-testid="request-private-dns"
              >
                {request?.status === "rejected"
                  ? "Ask again"
                  : "Request access"}
              </Button>
            )}
            {createRequest.isError && (
              <ApiErrorAlert
                error={createRequest.error}
                fallback="Failed to send request"
              />
            )}
          </>
        ) : (
          <>
            <Text as="p" size="sm" className="text-ink-3">
              Route this device's DNS through Wardnet everywhere — at home and
              roaming. Follow the steps for your phone.
            </Text>
            <PrivateDnsInstructions
              hostname={hostname}
              profileUrl={privateDnsService.profileUrl()}
              variant="on-device"
            />
          </>
        )}
      </CardContent>
    </Card>
  );
}

/**
 * Settings tab — self-service controls for the calling device.
 *
 * The DNS capture toggle flips the device's own `dns_capture_enabled` flag
 * (resolved by IP, no login). Retention caps are admin-owned and shown
 * read-only. The notifications toggle registers a device-keyed Web Push
 * subscription (#594).
 */
export default function Settings() {
  const { data, isLoading } = useMyDevice();
  const setCapture = useSetMyCaptureEnabled();

  const device = data?.device;

  if (isLoading) {
    return (
      <Text as="p" size="sm" className="p-5 text-ink-3">
        Loading…
      </Text>
    );
  }

  if (!device) {
    return (
      <div className="flex flex-col items-center gap-4 px-5 py-16 text-center">
        <WifiOffIcon className="size-12 text-ink-3/50" />
        <Text as="h1" size="lg" weight="semibold" className="text-ink">
          Device not detected
        </Text>
        <Text as="p" size="sm" className="max-w-md text-ink-3">
          Your device has not been detected on the network yet. Make sure you
          are accessing Wardnet directly from the local network.
        </Text>
      </div>
    );
  }

  const enabled = device.dns_capture_enabled;

  return (
    <div className="flex flex-col gap-6 p-5">
      <Text as="h1" size="lg" weight="semibold" className="text-ink">
        Settings
      </Text>

      <Card>
        <CardHeader>
          <CardTitle>DNS capture</CardTitle>
          <span className="ml-auto">
            <Toggle
              checked={enabled}
              onCheckedChange={(next) => setCapture.mutate(next)}
              disabled={setCapture.isPending}
              aria-label="Enable DNS capture"
              data-testid="capture-toggle"
            />
          </span>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <Text as="p" size="sm" className="text-ink-3">
            Wardnet will capture your device's DNS activity and sync it to this
            device so you can see your own stats. Data stays on this device - it
            is not sent anywhere else.
          </Text>

          <Text
            as="div"
            size="xs"
            className="rounded-lg bg-sunken px-3 py-2 text-ink-3"
          >
            Your administrator keeps up to{" "}
            <Text as="span" weight="medium" className="text-ink">
              {device.dns_capture_cap_count.toLocaleString()}
            </Text>{" "}
            events for{" "}
            <Text as="span" weight="medium" className="text-ink">
              {device.dns_capture_cap_days}
            </Text>{" "}
            {device.dns_capture_cap_days === 1 ? "day" : "days"} on the gateway
            before syncing. This retention limit is set by your administrator.
          </Text>

          {setCapture.isError && (
            <ApiErrorAlert
              error={setCapture.error}
              fallback="Failed to update DNS capture"
            />
          )}
        </CardContent>
      </Card>

      <MyRequests />

      <Notifications />

      <PrivateDns />
    </div>
  );
}
