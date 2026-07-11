import type { TlsStatusResponse } from "@wardnet/js";
import { formatDate, Text } from "@wardnet/web";

/**
 * Presentational view of the daemon's coarse TLS provisioning phase. Shared by
 * the setup wizard's remote-access step and the dashboard provisioning
 * indicator so both render progress identically. Renders nothing for the
 * `idle` phase.
 */
export function RemoteAccessProgress({
  status,
}: {
  status: TlsStatusResponse;
}) {
  const { phase, domain, not_after, error } = status;

  if (phase === "issuing") {
    return (
      <Text
        as="div"
        size="sm"
        className="rounded-md border border-line bg-sunken p-4"
      >
        <Text as="p" weight="medium" className="text-ink">
          Issuing certificate…
        </Text>
        <Text as="p" className="mt-1 text-ink-3">
          {domain ? (
            <>
              Provisioning HTTPS for <span className="font-mono">{domain}</span>
              . This can take a minute - publishing the DNS challenge and
              waiting for Let's Encrypt.
            </>
          ) : (
            "Provisioning HTTPS. This can take a minute."
          )}
        </Text>
      </Text>
    );
  }

  if (phase === "issued") {
    return (
      <Text
        as="div"
        size="sm"
        className="rounded-md border border-line bg-accent-soft p-4"
      >
        <Text as="p" weight="medium" className="text-accent-soft-ink">
          Remote access is live
        </Text>
        <Text as="p" className="mt-1 text-ink-2">
          {domain && (
            <>
              <span className="font-mono">{domain}</span> has a valid
              certificate
              {not_after && <> until {formatDate(not_after)}</>}.
            </>
          )}
        </Text>
      </Text>
    );
  }

  if (phase === "failed") {
    return (
      <Text
        as="div"
        size="sm"
        className="rounded-md border border-line bg-danger-soft p-4"
      >
        <Text as="p" weight="medium" className="text-danger-soft-ink">
          Certificate issuance failed
        </Text>
        {/* line-clamp-2 + min-h-10: `error` is an upstream ACME/HTTP error
            string of unbounded length - clamped so a long one can't push the
            rest of the page (e.g. the dashboard's stat grid below this
            banner) around; the full text remains in the DOM for assistive
            tech. The min-height pins this to exactly 2 lines' worth of space
            even when the (possibly short) text only wraps to 1 - a
            content-dependent 1-vs-2-line height wouldn't be a stable target
            for the e2e dashboard visual test to mask. */}
        <Text as="p" className="mt-1 line-clamp-2 min-h-10 text-ink-2">
          {error ?? "The daemon could not issue a certificate."} You can retry
          later from Settings - the daemon also retries automatically.
        </Text>
      </Text>
    );
  }

  return null;
}
