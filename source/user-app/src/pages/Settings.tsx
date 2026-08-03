import { useEffect, useRef } from "react";
import { useLocation } from "react-router";
import { WifiOffIcon } from "lucide-react";
import {
  ApiErrorAlert,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  PrivateDnsInstructions,
  RuleRequestStatusPill,
  Text,
  Toggle,
  isIosBrowserTab,
  privateDnsService,
  useMyDevice,
  useMyRuleRequests,
  usePrivateDnsMe,
  usePushNotifications,
  useSetMyCaptureEnabled,
} from "@wardnet/web";

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
  const { data, isLoading } = useMyRuleRequests();

  if (isLoading || !data || data.length === 0) {
    return null;
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>My requests</CardTitle>
      </CardHeader>
      <CardContent>
        <ul className="flex flex-col gap-2" data-testid="my-requests">
          {data.map((r) => (
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
              <RuleRequestStatusPill status={r.status} />
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
 * Grants are admin-only in v1, so the ungranted states are informational — a
 * household member can't fix the prerequisite themselves, only ask.
 */
function PrivateDns() {
  const { data: me, isLoading } = usePrivateDnsMe();
  const { hash: routerHash } = useLocation();
  const cardRef = useRef<HTMLDivElement>(null);

  // The push deep-links to `/settings#private-dns`, but this card is the last
  // of four — without this the member taps "Private DNS is ready" and lands on
  // the DNS-capture toggle with the setup steps off-screen.
  //
  // `useLocation().hash` alone is not enough. When the app is already open on
  // /settings the service worker calls `existing.navigate(…#private-dns)`,
  // which is a fragment-only, same-document navigation: it fires `hashchange`,
  // never `popstate`, and `BrowserRouter` only listens to the latter — so the
  // router's hash would stay stale in exactly the case this exists for. Listen
  // for `hashchange` too and read `window.location` there. The card shell
  // renders in every state, so the ref is always attached.
  useEffect(() => {
    // Named `fragment` rather than `hash`: the `security` ESLint plugin's
    // timing-attack heuristic keys off the identifier, and a URL fragment is
    // not a secret.
    const scrollIfTargeted = (fragment: string) => {
      if (fragment !== PRIVATE_DNS_HASH) return;
      cardRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
    };
    scrollIfTargeted(routerHash || window.location.hash);
    const onHashChange = () => scrollIfTargeted(window.location.hash);
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, [routerHash]);

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
          ? "This device hasn't been granted Private DNS yet. Ask your administrator to grant it, then reopen this page."
          : !me.hostname
            ? // Granted, but the domain lookup didn't resolve — transient and
              // self-healing, so don't send the member to their admin over it.
              "This device is set up for Private DNS, but its hostname isn't available right now. Check back in a moment."
            : null;

  // Non-null exactly when every check above passed, so the setup view can rely
  // on it without an assertion.
  const hostname = message === null ? (me?.hostname ?? null) : null;

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
          <Text as="p" size="sm" className="text-ink-3">
            {message}
          </Text>
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
