import type {
  DdnsResolutionCheckResponse,
  DdnsResolutionVerdict,
  DdnsStatusResponse,
  TlsStatusResponse,
} from "@wardnet/js";
import { formatDate, timeAgo, Text } from "@wardnet/web";
import { RemoteAccessProgress } from "@/components/features/RemoteAccessProgress";

/**
 * Shared presentational view of remote-access status. Driven entirely by props so
 * the same labels, verdict wording, and cert formatting are reused across
 * surfaces and can't drift:
 *
 * - `variant="banner"` renders only the compact provisioning-phase progress
 *   (delegating to {@link RemoteAccessProgress}); the dashboard banner uses it.
 * - `variant="full"` adds the steady-state detail — provider, public hostname,
 *   certificate expiry, and the external resolution check — laid out as a
 *   definition-list grid matching the rest of the admin site's status cards.
 *
 * Purely presentational and action-free: the consuming page owns the card
 * chrome and any buttons (re-check, change provider) via the card header/footer.
 */
export interface RemoteAccessStatusProps {
  tls: TlsStatusResponse;
  variant: "banner" | "full";
  ddns?: DdnsStatusResponse;
  resolution?: DdnsResolutionCheckResponse;
  /** Epoch-ms of the last successful resolution check (react-query `dataUpdatedAt`).
   *  Rendered as a "Checked …" line so a re-check is visible even when the verdict
   *  is unchanged. */
  lastCheckedAt?: number;
}

/** Human label + tone for each resolution verdict. */
function verdictView(verdict: DdnsResolutionVerdict): {
  label: string;
  tone: string;
} {
  switch (verdict) {
    case "match":
      return { label: "Resolves correctly", tone: "text-accent-soft-ink" };
    case "mismatch":
      return { label: "Points to the wrong IP", tone: "text-danger-soft-ink" };
    case "pending":
      return { label: "Not yet visible publicly", tone: "text-ink-3" };
    case "not_configured":
      return { label: "Not configured", tone: "text-ink-3" };
  }
}

function providerLabel(provider: string | null | undefined): string {
  if (provider === "wardnet") return "Wardnet";
  if (provider === "cloudflare") return "Your domain (Cloudflare)";
  return "—";
}

export function RemoteAccessStatus({
  tls,
  variant,
  ddns,
  resolution,
  lastCheckedAt,
}: RemoteAccessStatusProps) {
  if (variant === "banner") {
    return <RemoteAccessProgress status={tls} />;
  }

  const verdict = resolution ? verdictView(resolution.verdict) : null;

  return (
    <div className="space-y-4">
      <RemoteAccessProgress status={tls} />

      <Text
        as="dl"
        size="sm"
        className="grid grid-cols-1 gap-x-8 gap-y-3 sm:grid-cols-2 lg:grid-cols-3"
      >
        <div>
          <dt className="text-ink-3">Public hostname</dt>
          <Text as="dd" size="xs" className="font-mono">
            {ddns?.fqdn ?? "—"}
          </Text>
        </div>
        <div>
          <dt className="text-ink-3">Provider</dt>
          <Text as="dd" weight="medium">
            {providerLabel(ddns?.provider)}
          </Text>
        </div>
        <div>
          <dt className="text-ink-3">Certificate</dt>
          <Text as="dd" weight="medium">
            {tls.not_after
              ? `Valid until ${formatDate(tls.not_after)}`
              : "Not issued yet"}
          </Text>
        </div>
        <div>
          <dt className="text-ink-3">Public DNS</dt>
          <dd className={verdict?.tone ?? "font-medium"}>
            {verdict ? verdict.label : "—"}
            {resolution && resolution.resolved_ips.length > 0 && (
              <Text as="span" size="xs" className="block font-mono text-ink-3">
                {resolution.resolved_ips.join(", ")}
              </Text>
            )}
            {lastCheckedAt ? (
              <Text
                as="span"
                size="xs"
                weight="normal"
                className="block text-ink-3"
              >
                Checked {timeAgo(new Date(lastCheckedAt).toISOString())}
              </Text>
            ) : null}
          </dd>
        </div>
      </Text>
    </div>
  );
}
