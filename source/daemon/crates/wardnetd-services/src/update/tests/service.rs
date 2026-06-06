use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{InstallUpdateRequest, UpdateConfigRequest};
use wardnet_common::auth::AuthContext;
use wardnet_common::update::{Release, UpdateChannel};
use wardnetd_data::repository::{UpdateHistoryRow, UpdateRepository};

use crate::auth_context;
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::update::applier::{BinaryApplier, SwapOutcome};
use crate::update::release_source::ReleaseSource;
use crate::update::service::{UpdateService, UpdateServiceImpl};
use crate::update::verifier::ReleaseVerifier;
use wardnet_common::update::{UpdateHistoryEntry, UpdateHistoryStatus};
use wardnetd_data::repository::SystemConfigRepository;

#[derive(Default)]
struct MemoryConfig {
    data: StdMutex<std::collections::HashMap<String, String>>,
}

#[async_trait]
impl SystemConfigRepository for MemoryConfig {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.data.lock().unwrap().get(key).cloned())
    }
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.data
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.data.lock().unwrap().remove(key);
        Ok(())
    }
    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(0)
    }
}

#[derive(Default)]
struct MemoryHistory {
    rows: StdMutex<Vec<UpdateHistoryEntry>>,
}

#[async_trait]
impl UpdateRepository for MemoryHistory {
    async fn insert(&self, row: &UpdateHistoryRow) -> anyhow::Result<i64> {
        let mut rows = self.rows.lock().unwrap();
        let id = i64::try_from(rows.len()).unwrap_or(i64::MAX) + 1;
        rows.push(UpdateHistoryEntry {
            id,
            from_version: row.from_version.clone(),
            to_version: row.to_version.clone(),
            phase: row.phase.clone(),
            status: row.status,
            error: row.error.clone(),
            started_at: chrono::Utc::now(),
            finished_at: None,
        });
        Ok(id)
    }
    async fn finalize(
        &self,
        id: i64,
        status: UpdateHistoryStatus,
        phase: &str,
        error: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(row) = rows.iter_mut().find(|r| r.id == id) {
            row.status = status;
            row.phase = phase.to_owned();
            row.error = error.map(ToOwned::to_owned);
            row.finished_at = Some(chrono::Utc::now());
        }
        Ok(())
    }
    async fn list(&self, limit: u32) -> anyhow::Result<Vec<UpdateHistoryEntry>> {
        let mut out = self.rows.lock().unwrap().clone();
        out.reverse();
        out.truncate(limit as usize);
        Ok(out)
    }
    async fn last_succeeded(&self) -> anyhow::Result<Option<UpdateHistoryEntry>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|r| r.status == UpdateHistoryStatus::Succeeded)
            .cloned())
    }
}

struct StubReleaseSource(Option<Release>);

#[async_trait]
impl ReleaseSource for StubReleaseSource {
    async fn latest(&self, _channel: UpdateChannel) -> anyhow::Result<Option<Release>> {
        Ok(self.0.clone())
    }
    async fn fetch_asset(&self, _url: &str) -> anyhow::Result<Vec<u8>> {
        Ok(Vec::new())
    }
}

struct AlwaysOkVerifier;

#[async_trait]
impl ReleaseVerifier for AlwaysOkVerifier {
    async fn verify_sha256(&self, _tarball: &[u8], _expected_hex: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn verify_signature(&self, _tarball: &[u8], _signature: &[u8]) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Default)]
struct RecordingApplier {
    apply_count: StdMutex<u32>,
    rolled_back: StdMutex<bool>,
    rollback_target: StdMutex<Option<std::path::PathBuf>>,
    apply_signatures: StdMutex<Vec<Vec<u8>>>,
    apply_should_fail: StdMutex<bool>,
}

#[async_trait]
impl BinaryApplier for RecordingApplier {
    async fn apply(&self, _tarball: &[u8], signature: &[u8]) -> anyhow::Result<SwapOutcome> {
        self.apply_signatures
            .lock()
            .unwrap()
            .push(signature.to_vec());
        if *self.apply_should_fail.lock().unwrap() {
            // Chained context simulates an `io::Error` wrapped with a path —
            // exercises the `format!("{err:#}")` branch in `run_install`.
            return Err(
                anyhow::anyhow!("EACCES (os error 13)").context("rename /usr/local/bin/wardnetd")
            );
        }
        *self.apply_count.lock().unwrap() += 1;
        let prev = std::path::PathBuf::from("/tmp/fake.old");
        *self.rollback_target.lock().unwrap() = Some(prev.clone());
        Ok(SwapOutcome {
            previous_binary: prev,
        })
    }
    async fn rollback(&self) -> anyhow::Result<()> {
        *self.rolled_back.lock().unwrap() = true;
        *self.rollback_target.lock().unwrap() = None;
        Ok(())
    }
    async fn rollback_available(&self) -> bool {
        self.rollback_target.lock().unwrap().is_some()
    }
}

fn test_admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

fn build_service(
    release: Option<Release>,
) -> (
    Arc<UpdateServiceImpl>,
    Arc<RecordingApplier>,
    Arc<BroadcastEventBus>,
) {
    build_service_full(release, false)
}

fn build_service_full(
    release: Option<Release>,
    require_signature: bool,
) -> (
    Arc<UpdateServiceImpl>,
    Arc<RecordingApplier>,
    Arc<BroadcastEventBus>,
) {
    let applier = Arc::new(RecordingApplier::default());
    let events = Arc::new(BroadcastEventBus::new(32));
    let history = Arc::new(MemoryHistory::default());
    let svc = Arc::new(UpdateServiceImpl::new(
        Arc::new(MemoryConfig::default()),
        history,
        Arc::new(StubReleaseSource(release)),
        Arc::new(AlwaysOkVerifier),
        applier.clone(),
        events.clone(),
        require_signature,
        "0.1.0",
        tokio_util::sync::CancellationToken::new(),
    ));
    (svc, applier, events)
}

fn test_release_with_minisig() -> Release {
    Release {
        version: "0.2.0".to_owned(),
        tarball_url: "http://example/t.tar.gz".to_owned(),
        sha256_url: "http://example/t.tar.gz.sha256".to_owned(),
        minisig_url: Some("http://example/t.tar.gz.minisig".to_owned()),
        published_at: None,
        notes: None,
    }
}

fn test_release_without_minisig() -> Release {
    Release {
        version: "0.2.0".to_owned(),
        tarball_url: "http://example/t.tar.gz".to_owned(),
        sha256_url: "http://example/t.tar.gz.sha256".to_owned(),
        minisig_url: None,
        published_at: None,
        notes: None,
    }
}

#[tokio::test]
async fn status_requires_admin() {
    let (svc, _, _) = build_service(None);
    let result = (svc as Arc<dyn UpdateService>).status().await;
    assert!(matches!(result, Err(crate::error::AppError::Forbidden(_))));
}

#[tokio::test]
async fn check_updates_last_known_version_and_emits_event() {
    let release = Release {
        version: "0.2.0".to_owned(),
        tarball_url: "http://example/t.tar.gz".to_owned(),
        sha256_url: "http://example/t.tar.gz.sha256".to_owned(),
        minisig_url: None,
        published_at: None,
        notes: None,
    };
    let (svc, _, events) = build_service(Some(release));
    let mut rx = events.subscribe();
    let svc_trait: Arc<dyn UpdateService> = svc;
    let status = auth_context::with_context(test_admin_ctx(), svc_trait.check())
        .await
        .unwrap();
    assert_eq!(status.status.latest_version.as_deref(), Some("0.2.0"));
    assert!(status.status.update_available);

    // An UpdateAvailable event should have been published.
    let evt = rx.try_recv().expect("expected UpdateAvailable event");
    assert!(matches!(
        evt,
        wardnet_common::event::WardnetEvent::UpdateAvailable { .. }
    ));
}

#[tokio::test]
async fn check_no_release_clears_latest() {
    let (svc, _, _) = build_service(None);
    let svc_trait: Arc<dyn UpdateService> = svc;
    let status = auth_context::with_context(test_admin_ctx(), svc_trait.check())
        .await
        .unwrap();
    assert_eq!(status.status.latest_version, None);
    assert!(!status.status.update_available);
}

#[tokio::test]
async fn install_rejects_downgrade() {
    let release = Release {
        version: "0.0.1".to_owned(),
        tarball_url: "http://example/t.tar.gz".to_owned(),
        sha256_url: "http://example/t.tar.gz.sha256".to_owned(),
        minisig_url: None,
        published_at: None,
        notes: None,
    };
    let (svc, _, _) = build_service(Some(release));
    let svc_trait: Arc<dyn UpdateService> = svc;
    let result = auth_context::with_context(
        test_admin_ctx(),
        svc_trait.install(InstallUpdateRequest::default()),
    )
    .await;
    assert!(matches!(result, Err(crate::error::AppError::BadRequest(_))));
}

#[tokio::test]
async fn update_config_persists_channel_and_auto_update() {
    let (svc, _, _) = build_service(None);
    let svc_trait: Arc<dyn UpdateService> = svc;
    let resp = auth_context::with_context(
        test_admin_ctx(),
        svc_trait.update_config(UpdateConfigRequest {
            auto_update_enabled: Some(true),
            channel: Some(UpdateChannel::Beta),
        }),
    )
    .await
    .unwrap();
    assert!(resp.status.auto_update_enabled);
    assert_eq!(resp.status.channel, UpdateChannel::Beta);
}

#[tokio::test]
async fn rollback_without_previous_fails() {
    let (svc, _, _) = build_service(None);
    let svc_trait: Arc<dyn UpdateService> = svc;
    let result = auth_context::with_context(test_admin_ctx(), svc_trait.rollback()).await;
    assert!(matches!(result, Err(crate::error::AppError::BadRequest(_))));
}

#[tokio::test]
async fn auto_install_skipped_when_disabled() {
    let release = Release {
        version: "0.2.0".to_owned(),
        tarball_url: "http://example/t.tar.gz".to_owned(),
        sha256_url: "http://example/t.tar.gz.sha256".to_owned(),
        minisig_url: None,
        published_at: None,
        notes: None,
    };
    let (svc, _, _) = build_service(Some(release));
    let svc_trait: Arc<dyn UpdateService> = svc;
    let result = auth_context::with_context(test_admin_ctx(), svc_trait.auto_install_if_due())
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn run_install_success_drives_pipeline_through_apply() {
    // End-to-end install: drives `run_install` → `install_pipeline` →
    // `applier.apply`. Covers the success path through the new
    // `Some(minisig_url)` branch and the `apply(&tarball, &sig_bytes)`
    // call introduced by the runner-mediated swap rework.
    let release = test_release_with_minisig();
    let (svc, applier, events) = build_service(Some(release.clone()));
    let mut rx = events.subscribe();

    auth_context::with_context(test_admin_ctx(), svc.run_install(release))
        .await
        .expect("install ok");

    assert_eq!(*applier.apply_count.lock().unwrap(), 1);
    let mut completed = false;
    while let Ok(evt) = rx.try_recv() {
        if matches!(
            evt,
            wardnet_common::event::WardnetEvent::UpdateCompleted { .. }
        ) {
            completed = true;
        }
    }
    assert!(completed, "expected UpdateCompleted event");
}

#[tokio::test]
async fn run_install_with_failing_apply_records_anyhow_chain() {
    // Regression for #309: the failing PathBuf propagated through the
    // anyhow chain must reach the history row's `error` column. The
    // `format!("{err:#}")` rendering walks the full chain — the bare
    // `to_string()` it replaced only produced the leaf message.
    let release = test_release_with_minisig();
    let (svc, applier, _events) = build_service(Some(release.clone()));
    *applier.apply_should_fail.lock().unwrap() = true;

    let err = auth_context::with_context(test_admin_ctx(), svc.run_install(release))
        .await
        .expect_err("expected install failure");

    let rendered = format!("{err:#}");
    assert!(
        rendered.contains("rename /usr/local/bin/wardnetd"),
        "expected outer context in chain, got: {rendered}",
    );
    assert!(
        rendered.contains("EACCES"),
        "expected inner cause in chain, got: {rendered}",
    );
}

#[tokio::test]
async fn install_pipeline_requires_minisig_when_signature_required() {
    // `require_signature=true` + a release without `minisig_url` must
    // bail in the Verifying phase. The runner-mediated swap also needs
    // a signature, but that constraint lives in the runner; this gate
    // is the daemon-side opt-in pre-stash check.
    let release = test_release_without_minisig();
    let (svc, _applier, _events) = build_service_full(Some(release.clone()), true);

    let (phase, err) = svc
        .install_pipeline(&release)
        .await
        .expect_err("expected err");
    assert!(matches!(
        phase,
        wardnet_common::update::InstallPhase::Verifying
    ));
    let rendered = format!("{err}");
    assert!(
        rendered.contains("signature required"),
        "expected signature-required message, got: {rendered}",
    );
}

#[tokio::test]
async fn install_pipeline_runs_signature_verifier_when_required() {
    // `require_signature=true` + a release with `minisig_url=Some`
    // drives the verifier's `verify_signature` method. Covers the
    // post-fetch verifier branch that's gated behind both flags.
    let release = test_release_with_minisig();
    let (svc, _applier, _events) = build_service_full(Some(release.clone()), true);

    svc.install_pipeline(&release)
        .await
        .expect("pipeline ok with require_signature + minisig_url");
}

#[tokio::test]
async fn install_pipeline_warns_and_proceeds_without_minisig_when_optional() {
    // `require_signature=false` + missing `minisig_url` must NOT abort:
    // the daemon stages the tarball with an empty signature and the
    // runner is the one that will refuse to swap (logged warning makes
    // that explicit). Empty `sig_bytes` reaches the applier verbatim.
    let release = test_release_without_minisig();
    let (svc, applier, _events) = build_service_full(Some(release.clone()), false);

    svc.install_pipeline(&release).await.expect("pipeline ok");

    let signatures = applier.apply_signatures.lock().unwrap();
    assert_eq!(signatures.len(), 1);
    assert!(
        signatures[0].is_empty(),
        "expected empty signature passthrough"
    );
}

#[tokio::test]
async fn rollback_with_previous_records_history_and_emits_log() {
    // Drives the success path of `rollback`: applier reports a
    // rollback target available, so the request marker is staged and
    // a `RolledBack` history row is recorded. Covers the rollback log
    // line introduced by the runner-mediated swap rework.
    let (svc, applier, _events) = build_service(None);
    // Pre-seed a rollback target the way a successful prior install would.
    *applier.rollback_target.lock().unwrap() = Some(std::path::PathBuf::from("/tmp/fake.old"));

    let svc_trait: Arc<dyn UpdateService> = svc;
    let resp = auth_context::with_context(test_admin_ctx(), svc_trait.rollback())
        .await
        .expect("rollback ok");

    assert!(resp.message.contains("rollback staged"));
    assert!(*applier.rolled_back.lock().unwrap());
}
