use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::dns_provider::DnsProvider;

const CF_API_BASE: &str = "https://api.cloudflare.com/client/v4";

/// TTL applied to user A records. 120 s is the Cloudflare free-tier minimum.
const A_RECORD_TTL: u32 = 120;
/// Short TTL for ACME TXT records so stale challenges expire quickly.
const TXT_RECORD_TTL: u32 = 60;

/// TCP connect timeout per attempt.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Total request timeout per attempt (headers + body).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Maximum number of retry attempts after the initial failure.
const MAX_RETRIES: u32 = 3;
/// Initial back-off before the first retry.
const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
/// Back-off ceiling — exponential doubling is capped here.
const MAX_BACKOFF: Duration = Duration::from_secs(10);

/// Cloudflare implementation of [`DnsProvider`].
///
/// Manages DNS records in a single Cloudflare zone via the v4 REST API.
/// The API token must be scoped to **DNS:Edit** on the target zone only —
/// never grant global zone permissions to this token.
///
/// All mutating requests are retried up to [`MAX_RETRIES`] times with
/// exponential back-off on network errors, 5xx server errors, and 429
/// rate-limit responses. 4xx client errors are returned immediately
/// without retrying.
#[derive(Debug)]
pub struct CloudflareDnsProvider {
    zone_id: String,
    http: reqwest::Client,
    /// Base URL for the Cloudflare REST API.
    /// Overridden in tests to point at a local mock server.
    base_url: String,
    /// Initial back-off duration before the first retry.
    /// Set to zero in tests so retry paths don't sleep.
    initial_backoff: Duration,
}

impl CloudflareDnsProvider {
    /// Build a provider from a Cloudflare API token and zone ID.
    ///
    /// The `Authorization: Bearer <token>` header is baked into a shared
    /// [`reqwest::Client`] and attached to every outbound request automatically.
    pub fn new(api_token: &str, zone_id: &str) -> anyhow::Result<Self> {
        use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};

        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {api_token}")).map_err(|_| {
            anyhow::anyhow!("Cloudflare API token contains invalid header characters")
        })?;
        headers.insert(AUTHORIZATION, auth_value);

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()?;

        Ok(Self {
            zone_id: zone_id.to_string(),
            http,
            base_url: CF_API_BASE.to_string(),
            initial_backoff: INITIAL_BACKOFF,
        })
    }

    /// Test-only constructor: points requests at `base_url` (a mock server)
    /// and uses zero back-off so retry paths run without sleeping.
    #[cfg(test)]
    pub(crate) fn new_for_test(
        api_token: &str,
        zone_id: &str,
        base_url: &str,
    ) -> anyhow::Result<Self> {
        use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};

        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {api_token}"))
            .map_err(|_| anyhow::anyhow!("invalid header value"))?;
        headers.insert(AUTHORIZATION, auth_value);
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;
        Ok(Self {
            zone_id: zone_id.to_string(),
            http,
            base_url: base_url.to_string(),
            initial_backoff: Duration::ZERO,
        })
    }

    fn records_url(&self) -> String {
        format!("{}/zones/{}/dns_records", self.base_url, self.zone_id)
    }

    fn record_url(&self, record_id: &str) -> String {
        format!(
            "{}/zones/{}/dns_records/{record_id}",
            self.base_url, self.zone_id
        )
    }

    /// Execute an HTTP request with retry + exponential back-off.
    ///
    /// `build` is called on every attempt, producing a fresh
    /// [`reqwest::RequestBuilder`] each time. This ensures the request body
    /// (serialised from an owned value in the caller) is re-serialised on
    /// retries without requiring `Clone` on the request.
    ///
    /// Retries on:
    /// - Network / transport errors
    /// - HTTP 5xx server errors
    /// - HTTP 429 Too Many Requests
    ///
    /// Returns immediately (without retrying) on 4xx client errors.
    async fn call<F>(&self, build: F) -> anyhow::Result<reqwest::Response>
    where
        F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
    {
        let mut backoff = self.initial_backoff;
        let mut last_err: anyhow::Error =
            anyhow::anyhow!("BUG: retry loop exited without setting last_err");

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                tracing::debug!(
                    attempt,
                    backoff_ms = backoff.as_millis(),
                    "retrying Cloudflare API call"
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }

            match build(&self.http).send().await {
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "Cloudflare API network error");
                    last_err = e.into();
                    // Network errors are always retried.
                }
                Ok(resp) => {
                    let status = resp.status();

                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                        tracing::warn!(attempt, "Cloudflare API rate-limited (429)");
                        last_err = anyhow::anyhow!("Cloudflare API: rate limited (429)");
                        continue;
                    }

                    if status.is_server_error() {
                        tracing::warn!(
                            attempt,
                            status = status.as_u16(),
                            "Cloudflare API server error"
                        );
                        last_err = anyhow::anyhow!("Cloudflare API: server error {status}");
                        if attempt < MAX_RETRIES {
                            continue;
                        }
                        return Err(last_err);
                    }

                    // 4xx and 2xx are returned to the caller as-is.
                    return Ok(resp);
                }
            }
        }

        Err(last_err)
    }

    async fn create_record(&self, body: &DnsRecordBody) -> anyhow::Result<String> {
        let url = self.records_url();
        let resp = self.call(|c| c.post(&url).json(body)).await?;
        parse_cf_response(resp).await
    }

    async fn update_record(&self, record_id: &str, body: &DnsRecordBody) -> anyhow::Result<String> {
        let url = self.record_url(record_id);
        let resp = self.call(|c| c.put(&url).json(body)).await?;
        parse_cf_response(resp).await
    }
}

#[async_trait]
impl DnsProvider for CloudflareDnsProvider {
    async fn upsert_a_record(
        &self,
        fqdn: &str,
        ip: &str,
        existing_record_id: Option<&str>,
    ) -> anyhow::Result<String> {
        let body = DnsRecordBody {
            r#type: "A".to_string(),
            name: fqdn.to_string(),
            content: ip.to_string(),
            ttl: A_RECORD_TTL,
            proxied: false,
        };
        match existing_record_id {
            Some(id) => self.update_record(id, &body).await,
            None => self.create_record(&body).await,
        }
    }

    async fn upsert_txt_record(
        &self,
        fqdn: &str,
        content: &str,
        existing_record_id: Option<&str>,
    ) -> anyhow::Result<String> {
        let body = DnsRecordBody {
            r#type: "TXT".to_string(),
            name: fqdn.to_string(),
            content: content.to_string(),
            ttl: TXT_RECORD_TTL,
            proxied: false,
        };
        match existing_record_id {
            Some(id) => self.update_record(id, &body).await,
            None => self.create_record(&body).await,
        }
    }

    async fn delete_record(&self, record_id: &str) -> anyhow::Result<()> {
        let url = self.record_url(record_id);
        let resp = self.call(|c| c.delete(&url)).await?;

        // 404 means the record is already absent — treat as success so the
        // caller can call delete idempotently without checking first.
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }

        resp.error_for_status()?;
        Ok(())
    }
}

// ── Cloudflare API response types ────────────────────────────────────────────

#[derive(Serialize)]
struct DnsRecordBody {
    r#type: String,
    name: String,
    content: String,
    ttl: u32,
    proxied: bool,
}

#[derive(Deserialize)]
struct DnsRecordResult {
    id: String,
}

#[derive(Deserialize)]
struct CfError {
    message: String,
}

#[derive(Deserialize)]
struct CfResponse {
    success: bool,
    errors: Vec<CfError>,
    result: Option<DnsRecordResult>,
}

async fn parse_cf_response(resp: reqwest::Response) -> anyhow::Result<String> {
    let status = resp.status();
    let body: CfResponse = resp.json().await.map_err(|e| {
        anyhow::anyhow!("Cloudflare API returned non-JSON body (HTTP {status}): {e}")
    })?;

    if !body.success {
        let msgs: Vec<_> = body.errors.iter().map(|e| e.message.as_str()).collect();
        anyhow::bail!("Cloudflare API error: {}", msgs.join("; "));
    }

    body.result
        .map(|r| r.id)
        .ok_or_else(|| anyhow::anyhow!("Cloudflare API returned success but no result.id"))
}
