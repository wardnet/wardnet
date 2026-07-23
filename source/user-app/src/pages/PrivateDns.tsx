import { ShieldOffIcon } from "lucide-react";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  PrivateDnsInstructions,
  Text,
  privateDnsService,
  usePrivateDnsMe,
} from "@wardnet/web";

/**
 * Private DNS setup for this device (issues #910/#916). Device-keyed like the
 * rest of the user PWA — the daemon identifies the device by source IP — so the
 * page just reflects `me()`: not-enabled, not-granted, or the per-platform setup
 * steps for the granted hostname. This is the page the admin's "Send to device"
 * notification opens.
 */
export default function PrivateDns() {
  const { data: me, isLoading } = usePrivateDnsMe();

  if (isLoading) {
    return (
      <Text as="p" size="sm" className="p-5 text-ink-3">
        Loading…
      </Text>
    );
  }

  // Narrow to a non-null hostname here so the setup view can rely on it; the
  // daemon only returns one when the feature is enabled *and* this device is
  // granted.
  const hostname = me?.enabled && me.granted ? me.hostname : null;

  if (!hostname) {
    return (
      <div className="flex flex-col items-center gap-4 px-5 py-16 text-center">
        <ShieldOffIcon className="size-12 text-ink-3/50" />
        <Text as="h1" size="lg" weight="semibold" className="text-ink">
          Private DNS not set up
        </Text>
        <Text as="p" size="sm" className="max-w-md text-ink-3">
          {me?.enabled
            ? "This device hasn't been granted Private DNS yet. Ask your administrator to grant it, then reopen this page."
            : "Private DNS isn't enabled on your network yet. Ask your administrator to turn it on."}
        </Text>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6 p-5">
      <Text as="h1" size="lg" weight="semibold" className="text-ink">
        Private DNS
      </Text>

      <Card>
        <CardHeader>
          <CardTitle>Set up encrypted DNS</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <Text as="p" size="sm" className="text-ink-3">
            Route this device's DNS through Wardnet everywhere — at home and
            roaming. Follow the steps for your phone.
          </Text>
          <PrivateDnsInstructions
            hostname={hostname}
            profileUrl={privateDnsService.profileUrl()}
          />
        </CardContent>
      </Card>
    </div>
  );
}
