/// Runtime configuration loaded from environment variables.
///
/// All required variables must be present at startup; the process exits
/// with a human-readable error if any are missing. Optional variables
/// fall back to documented defaults.
///
/// # Multi-region layout
///
/// Each bridge deployment is bound to one region. The `region` label and
/// `subdomain_parent` together define the namespace this instance owns:
///
/// | Variable | Example (US) | Example (EU) |
/// |---|---|---|
/// | `REGION` | `us` | `eu` |
/// | `SUBDOMAIN_PARENT` | `my.us.wardnet.network` | `my.eu.wardnet.network` |
///
/// Devices registered here receive subdomains under `SUBDOMAIN_PARENT`.
/// There is no cross-region record synchronisation — the Pi selects a
/// region at setup time and remains bound to that bridge instance.
#[derive(Debug, Clone)]
pub struct Config {
    /// TCP address to listen on for the HTTP API. Defaults to `127.0.0.1:8080`.
    pub listen_addr: String,

    /// `MySQL` DSN, e.g. `mysql://user:pass@host:3306/wardnet`.
    pub database_url: String,

    /// Cloudflare API token scoped to DNS:Edit on the `cloudflare_zone_id` zone only.
    pub cloudflare_api_token: String,

    /// Cloudflare zone ID that owns `subdomain_parent`
    /// (e.g. the zone for `wardnet.network`).
    pub cloudflare_zone_id: String,

    /// Short region label used in API responses and log fields,
    /// e.g. `"us"` or `"eu"`. Returned in `RegisterResponse` so the Pi
    /// knows which bridge region it is bound to.
    pub region: String,

    /// DNS parent domain under which user subdomains are created,
    /// e.g. `"my.us.wardnet.network"`. Must be inside the zone owned by
    /// `cloudflare_zone_id`.
    pub subdomain_parent: String,

    /// TCP address for the SNI-routing HTTPS listener. Defaults to `0.0.0.0:443`.
    ///
    /// The SNI demuxer reads the TLS `ClientHello` without terminating TLS and
    /// routes connections to either Caddy (for the bridge hostname) or to the
    /// reverse-tunnel router (for install subdomains).
    pub sni_listen_addr: String,

    /// TCP address for the SNI-routing DNS-over-TLS listener. Defaults to `0.0.0.0:853`.
    ///
    /// Same SNI passthrough logic as the HTTPS listener; used for Android
    /// Private DNS.
    pub dot_listen_addr: String,

    /// Local address of the Caddy reverse-proxy, e.g. `127.0.0.1:8443`.
    ///
    /// TLS connections whose SNI hostname matches `bridge_hostname` are
    /// forwarded here unchanged (TLS passthrough); Caddy handles the
    /// `bridge.<region>.wardnet.network` certificate.
    pub caddy_addr: String,

    /// Public hostname of this bridge instance,
    /// e.g. `"bridge.us.wardnet.network"`. Connections with this SNI are
    /// routed to Caddy; all others are assumed to be install traffic.
    pub bridge_hostname: String,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// # Errors
    /// Returns an error if any required variable is absent.
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            listen_addr: std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            database_url: required("DATABASE_URL")?,
            cloudflare_api_token: required("CLOUDFLARE_API_TOKEN")?,
            cloudflare_zone_id: required("CLOUDFLARE_ZONE_ID")?,
            region: required("REGION")?,
            subdomain_parent: required("SUBDOMAIN_PARENT")?,
            sni_listen_addr: std::env::var("SNI_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:443".to_string()),
            dot_listen_addr: std::env::var("DOT_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:853".to_string()),
            caddy_addr: std::env::var("CADDY_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8443".to_string()),
            bridge_hostname: required("BRIDGE_HOSTNAME")?,
        })
    }

    /// Construct the fully-qualified domain name for an install's A record.
    ///
    /// `"happy-einstein"` → `"happy-einstein.my.us.wardnet.network"`
    #[must_use]
    pub fn install_fqdn(&self, name: &str) -> String {
        format!("{name}.{}", self.subdomain_parent)
    }

    /// Construct the FQDN for an install's ACME DNS-01 TXT record.
    ///
    /// `"happy-einstein"` → `"_acme-challenge.happy-einstein.my.us.wardnet.network"`
    #[must_use]
    pub fn acme_fqdn(&self, name: &str) -> String {
        format!("_acme-challenge.{name}.{}", self.subdomain_parent)
    }
}

fn required(key: &str) -> anyhow::Result<String> {
    std::env::var(key)
        .map_err(|_| anyhow::anyhow!("required environment variable `{key}` is not set"))
}

#[cfg(test)]
mod tests;
