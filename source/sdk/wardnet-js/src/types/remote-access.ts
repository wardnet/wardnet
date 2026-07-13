// Remote access — DDNS + TLS provisioning (wardnet-cloud enrollment flow).

/** Request body for `POST /api/ddns/enrollment-code` (wardnet provider, step 1). */
export interface DdnsEnrollmentCodeRequest {
  /** The wardnet account email a one-time enrollment code is emailed to. */
  email: string;
}

/** Request body for `POST /api/ddns/enroll` (wardnet provider, step 2). */
export interface DdnsEnrollRequest {
  /** The one-time code emailed to the account, as entered by the operator. */
  code: string;
}

/** Response for `GET /api/ddns/check`. */
export interface DdnsCheckResponse {
  /** `true` when the slug is well-formed and unclaimed. */
  available: boolean;
}

/** Request body for `POST /api/ddns/register` (wardnet provider, final step). */
export interface DdnsRegisterRequest {
  /**
   * The vanity slug to claim, e.g. `happy-einstein`, forming
   * `<slug>.my.wardnet.services`. The cloud validates it (3–32 chars,
   * `[a-z0-9-]`, no leading/trailing hyphen, not reserved).
   */
  slug: string;
  /** Optional human-facing network name; defaults to the slug when omitted. */
  display_name?: string;
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
  /** The public hostname now assigned to this network. */
  fqdn: string;
  /** The region slug (display only); `null` for BYOD-Cloudflare. */
  region: string | null;
  /**
   * `true` when the daemon JOINED an already-existing network of this account
   * (re-enrollment with a slug it already owns) instead of creating a fresh
   * one — the network's region and name won over the request.
   */
  adopted: boolean;
}

/** Response for `GET /api/ddns/status`. */
export interface DdnsStatusResponse {
  /** `null` when DDNS is not configured; otherwise `"wardnet"` or `"cloudflare"`. */
  provider: string | null;
  /** The active public hostname (wardnet subdomain or BYOD domain), if any. */
  fqdn: string | null;
  /** The IP last published by the daemon, if any. */
  last_public_ip: string | null;
  /**
   * `true` when the wardnet subscription is suspended — the premium app
   * surfaces (user PWA + admin mobile app) are disabled until it is restored.
   * Always `false` for BYOD-Cloudflare.
   */
  suspended: boolean;
}

/**
 * Verdict of the external resolution check — whether public DNS resolves the
 * canonical FQDN to the IP the daemon published, bypassing the local override.
 */
export type DdnsResolutionVerdict = "not_configured" | "match" | "mismatch" | "pending";

/** Response for `GET /api/ddns/resolution-check`. */
export interface DdnsResolutionCheckResponse {
  /** Overall verdict. */
  verdict: DdnsResolutionVerdict;
  /** The canonical FQDN that was queried, if DDNS is configured. */
  fqdn: string | null;
  /** The IP the daemon last published (what public DNS is expected to return). */
  expected_ip: string | null;
  /** The A records the external resolver actually returned. */
  resolved_ips: string[];
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
