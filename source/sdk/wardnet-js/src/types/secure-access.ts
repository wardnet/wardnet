// Secure access — DDNS + TLS provisioning (issues #527–#530).

/** Response for `GET /api/ddns/check`. */
export interface DdnsCheckResponse {
  /** `true` when the name is well-formed and unclaimed on the best bridge. */
  available: boolean;
}

/** Request body for `POST /api/ddns/register` (bridge provider). */
export interface DdnsRegisterRequest {
  /** The short name to claim, e.g. `happy-einstein`. */
  name: string;
}

/** Request body for `POST /api/ddns/cloudflare` (BYOD provider). */
export interface ConfigureCloudflareRequest {
  /** A Cloudflare API token scoped to DNS:Edit on the domain's zone. */
  token: string;
  /** The fully-qualified domain the operator controls, e.g. `home.example.com`. */
  domain: string;
}

/** Response for `POST /api/ddns/register` and `POST /api/ddns/cloudflare`. */
export interface DdnsRegisterResponse {
  /** The public hostname now assigned to this installation. */
  fqdn: string;
  /** The bridge region label (display only); `null` for BYOD-Cloudflare. */
  region: string | null;
}

/** Response for `GET /api/ddns/status`. */
export interface DdnsStatusResponse {
  /** `null` when DDNS is not configured; otherwise `"bridge"` or `"cloudflare"`. */
  provider: string | null;
  /** The active public hostname (bridge subdomain or BYOD domain), if any. */
  fqdn: string | null;
  /** The IP last published by the daemon, if any. */
  last_public_ip: string | null;
}

/**
 * Coarse stage of the daemon's TLS-certificate provisioning. Mirrors the
 * daemon's persisted phase so the wizard and dashboard can show live progress.
 */
export type TlsProvisioningPhase = "idle" | "issuing" | "issued" | "failed";

/** Response for `GET /api/tls/status`. */
export interface TlsStatusResponse {
  /** Current coarse provisioning phase. */
  phase: TlsProvisioningPhase;
  /** The domain being (or already) provisioned, if any. */
  domain: string | null;
  /** RFC 3339 expiry of the stored certificate, when one has been issued. */
  not_after: string | null;
  /** Human-readable error from the last failed attempt, when `phase` is `failed`. */
  error: string | null;
}
