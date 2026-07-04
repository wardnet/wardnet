import { WifiOffIcon } from "lucide-react";
import {
  ApiErrorAlert,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  RuleRequestStatusPill,
  Text,
  Toggle,
  isIosBrowserTab,
  useMyDevice,
  useMyRuleRequests,
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
            device so you can see your own stats. Data stays on this device — it
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
    </div>
  );
}
