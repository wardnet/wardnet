//! HTTPS-backed [`ReleaseSource`] implementation.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use wardnet_common::update::{Release, UpdateChannel};

use crate::update::release_source::ReleaseSource;

/// Shape of `<channel>.json` emitted by the marketing site's build step.
///
/// Mirrors [`source/marketing-site/scripts/generate-release-manifests.ts`]. Only the
/// fields the runner actually consumes are modelled — the rest are ignored.
#[derive(Debug, Deserialize)]
struct ManifestJson {
    version: String,
    #[serde(default)]
    asset_base_url: String,
    #[serde(default)]
    binary: Option<ManifestBinary>,
    #[serde(default)]
    published_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    notes_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ManifestBinary {
    #[allow(dead_code)]
    name: String,
}

/// Fetches release manifests and asset bytes from an HTTPS base URL.
pub struct HttpsManifestSource {
    base_url: String,
    target_arch: String,
    client: Client,
}

impl HttpsManifestSource {
    /// Construct a new manifest source. `base_url` is the manifest root
    /// (e.g. `https://releases.wardnet.network`). `http_timeout` caps each
    /// fetch; `target_arch` is the short arch name embedded in asset
    /// filenames (`aarch64`, `x86_64`).
    pub fn new(
        base_url: impl Into<String>,
        target_arch: impl Into<String>,
        http_timeout: Duration,
    ) -> anyhow::Result<Self> {
        let client = Client::builder().timeout(http_timeout).build()?;
        Ok(Self {
            base_url: base_url.into(),
            target_arch: target_arch.into(),
            client,
        })
    }

    fn manifest_url(&self, channel: UpdateChannel) -> String {
        format!(
            "{}/{}.json",
            self.base_url.trim_end_matches('/'),
            channel.as_str()
        )
    }

    /// Compute the expected tarball name for this build's architecture.
    ///
    /// The release workflow stages one tarball per target under a shared
    /// `asset_base_url`; the manifest only advertises one of them (today the
    /// primary arm64 build), so the daemon has to pick the asset matching
    /// its own arch by convention: `wardnetd-<version>-<arch>.tar.gz`.
    fn tarball_name(&self, version: &str) -> String {
        format!("wardnetd-{version}-{arch}.tar.gz", arch = self.target_arch)
    }
}

#[async_trait]
impl ReleaseSource for HttpsManifestSource {
    async fn latest(&self, channel: UpdateChannel) -> anyhow::Result<Option<Release>> {
        let url = self.manifest_url(channel);
        tracing::debug!(url = %url, "fetching release manifest: url={url}");
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "manifest fetch failed: status={} url={url}",
                response.status()
            ));
        }
        let manifest: ManifestJson = response.json().await?;
        if manifest.version.is_empty() || manifest.binary.is_none() {
            // Empty placeholder emitted when no release exists for the channel.
            return Ok(None);
        }

        let base = manifest.asset_base_url.trim_end_matches('/').to_owned();
        let tarball_name = self.tarball_name(&manifest.version);
        let tarball_url = format!("{base}/{tarball_name}");
        let sha256_url = format!("{tarball_url}.sha256");
        let minisig_url = format!("{tarball_url}.minisig");

        Ok(Some(Release {
            version: manifest.version,
            tarball_url,
            sha256_url,
            minisig_url: Some(minisig_url),
            published_at: manifest.published_at,
            notes: manifest.notes_url,
        }))
    }

    async fn fetch_asset(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        tracing::debug!(url = %url, "fetching release asset: url={url}");
        let response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "asset fetch failed: status={} url={url}",
                response.status()
            ));
        }
        Ok(response.bytes().await?.to_vec())
    }
}
