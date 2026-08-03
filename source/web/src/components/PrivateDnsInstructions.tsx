import { Text } from "@wardnet/ui";
import { CopyButton } from "./CopyButton";
import { InboundWgQrCode } from "./InboundWgQrCode";

/**
 * Who is reading the instructions, relative to the phone being set up.
 *
 * - `"remote"` — a different screen than the target device (the admin site's
 *   granted modal, on a desktop). The QR is the bridge to the phone.
 * - `"on-device"` — the target phone itself (the user PWA). A QR would ask the
 *   phone to scan its own screen, so the profile link is the only path.
 */
export type PrivateDnsInstructionsVariant = "remote" | "on-device";

interface PrivateDnsInstructionsProps {
  /** The device's full secret hostname (`<token>.<domain>`). */
  hostname: string;
  /**
   * URL of this device's signed iOS profile. May be app-relative (the SDK's
   * `profileUrl()` returns `/api/…`); it is resolved to an absolute URL so the
   * QR is scannable and the link works from the phone.
   */
  profileUrl: string;
  /** Defaults to `"remote"` — see {@link PrivateDnsInstructionsVariant}. */
  variant?: PrivateDnsInstructionsVariant;
  className?: string;
}

/**
 * Per-platform Private DNS setup steps, shared between the admin-site granted
 * modal and the user PWA (issues #915/#916) so both render identically.
 *
 * Android can't be configured programmatically — the hostname is pasted by hand
 * into Settings, hence the copy button, in both variants. iOS installs a
 * downloadable configuration profile, so it gets a link, plus a QR when the
 * reader is on a different screen than the phone. The profile endpoint is keyed
 * by the requesting device's source IP, so the QR only yields the correct
 * profile when the granted phone scans it on-LAN.
 */
export function PrivateDnsInstructions({
  hostname,
  profileUrl,
  variant = "remote",
  className,
}: PrivateDnsInstructionsProps) {
  const profileHref = absoluteUrl(profileUrl);
  const showQr = variant === "remote";

  return (
    <div
      className={["flex flex-col gap-6", className ?? ""].join(" ")}
      data-testid="private-dns-instructions"
    >
      <section className="flex flex-col gap-3">
        <Text as="h4" size="sm" weight="medium" className="text-ink">
          Android
        </Text>
        <div className="flex items-center gap-2 rounded-lg border border-line bg-sunken px-3 py-2">
          <Text
            as="code"
            size="sm"
            className="flex-1 truncate font-mono text-ink"
            data-testid="private-dns-hostname"
          >
            {hostname}
          </Text>
          <CopyButton
            value={hostname}
            data-testid="private-dns-copy-hostname"
          />
        </div>
        <div className="mt-1 flex flex-col gap-1">
          <Text as="p" size="sm" className="text-ink-3">
            1. Open Settings → Network &amp; internet → Private DNS.
          </Text>
          <Text as="p" size="sm" className="text-ink-3">
            2. Choose “Private DNS provider hostname”.
          </Text>
          <Text as="p" size="sm" className="text-ink-3">
            3. Paste the hostname above and save.
          </Text>
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <Text as="h4" size="sm" weight="medium" className="text-ink">
          iPhone &amp; iPad
        </Text>
        <div className="flex flex-col items-center gap-3">
          {showQr && (
            <InboundWgQrCode
              value={profileHref}
              size={180}
              alt="Private DNS configuration profile QR code"
            />
          )}
          {/* Opens out of the app on purpose. In `on-device` this link is the
              only iOS path, and an installed standalone PWA navigating its own
              webview to a `.mobileconfig` tends to show a blank view or drop
              the tap entirely — breaking out to Safari is where the "Profile
              Downloaded" banner reliably appears. */}
          <a
            href={profileHref}
            target="_blank"
            rel="noreferrer"
            className="text-sm text-accent hover:underline"
            data-testid="private-dns-profile-link"
          >
            Download configuration profile
          </a>
        </div>
        <div className="mt-1 flex flex-col gap-1">
          <Text as="p" size="sm" className="text-ink-3">
            {showQr
              ? "1. Scan the QR with the iPhone camera, or tap the link on the device itself."
              : "1. Tap the link above to download the profile."}
          </Text>
          <Text as="p" size="sm" className="text-ink-3">
            2. Open Settings — a “Profile Downloaded” banner appears near the
            top.
          </Text>
          <Text as="p" size="sm" className="text-ink-3">
            3. Tap Install and confirm.
          </Text>
        </div>
      </section>
    </div>
  );
}

/**
 * Resolve a possibly-relative URL against the current origin so the QR encodes
 * something a phone can actually reach. Degrades to the input when there is no
 * `window` (SSR) or the URL can't be parsed.
 */
function absoluteUrl(url: string): string {
  if (typeof window === "undefined") return url;
  try {
    return new URL(url, window.location.origin).href;
  } catch {
    return url;
  }
}
