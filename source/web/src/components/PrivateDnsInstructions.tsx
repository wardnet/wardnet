import { Text } from "@wardnet/ui";
import { CopyButton } from "./CopyButton";
import { InboundWgQrCode } from "./InboundWgQrCode";

interface PrivateDnsInstructionsProps {
  /** The device's full secret hostname (`<token>.<domain>`). */
  hostname: string;
  /**
   * URL of this device's signed iOS profile. May be app-relative (the SDK's
   * `profileUrl()` returns `/api/…`); it is resolved to an absolute URL so the
   * QR is scannable and the link works from the phone.
   */
  profileUrl: string;
  className?: string;
}

/**
 * Per-platform Private DNS setup steps, shared between the admin-site granted
 * modal and the user PWA (issues #915/#916) so both render identically.
 *
 * Android can't be configured programmatically — the hostname is pasted by hand
 * into Settings, hence the copy button. iOS installs a downloadable
 * configuration profile, so it gets a link + a QR the phone can scan. The
 * profile endpoint is keyed by the requesting device's source IP, so the QR
 * only yields the correct profile when the granted phone scans it on-LAN.
 */
export function PrivateDnsInstructions({
  hostname,
  profileUrl,
  className,
}: PrivateDnsInstructionsProps) {
  const profileHref = absoluteUrl(profileUrl);

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
          <InboundWgQrCode
            value={profileHref}
            size={180}
            alt="Private DNS configuration profile QR code"
          />
          <a
            href={profileHref}
            className="text-sm text-accent hover:underline"
            data-testid="private-dns-profile-link"
          >
            Download configuration profile
          </a>
        </div>
        <div className="mt-1 flex flex-col gap-1">
          <Text as="p" size="sm" className="text-ink-3">
            1. Scan the QR with the iPhone camera, or tap the link on the device
            itself.
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
