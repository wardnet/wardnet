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
      <Text as="div" size="sm" className="rounded-md border border-line bg-sunken p-4">
        <Text as="p" weight="medium" className="text-ink">
          Issuing certificate…
        </Text>
        <p className="mt-1 text-ink-3">
          {domain ? (
            <>
              Provisioning HTTPS for <span className="font-mono">{domain}</span>
              . This can take a minute — publishing the DNS challenge and
              waiting for Let's Encrypt.
            </>
          ) : (
            "Provisioning HTTPS. This can take a minute."
          )}
        </p>
      </Text>
    );
  }

  if (phase === "issued") {
    return (
      <Text as="div" size="sm" className="rounded-md border border-line bg-accent-soft p-4">
        <Text as="p" weight="medium" className="text-accent-soft-ink">
          Remote access is live
        </Text>
        <p className="mt-1 text-ink-2">
          {domain && (
            <>
              <span className="font-mono">{domain}</span> has a valid
              certificate
              {not_after && <> until {formatDate(not_after)}</>}.
            </>
          )}
        </p>
      </Text>
    );
  }

  if (phase === "failed") {
    return (
      <Text as="div" size="sm" className="rounded-md border border-line bg-danger-soft p-4">
        <Text as="p" weight="medium" className="text-danger-soft-ink">
          Certificate issuance failed
        </Text>
        <p className="mt-1 text-ink-2">
          {error ?? "The daemon could not issue a certificate."} You can retry
          later from Settings — the daemon also retries automatically.
        </p>
      </Text>
    );
  }

  return null;
}
