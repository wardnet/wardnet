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
    /// TCP address to listen on. Defaults to `127.0.0.1:8080`.
    ///
    /// Defaults to loopback — the bridge should always sit behind a reverse
    /// proxy (Caddy in production). Binding to `0.0.0.0` would expose the
    /// unauthenticated endpoints directly on every interface.
    pub listen_addr: String,

    /// `SQLite` database path (file) or `":memory:"` for tests.
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
