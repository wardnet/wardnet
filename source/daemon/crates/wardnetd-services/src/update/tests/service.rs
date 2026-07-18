use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::api::{InstallUpdateRequest, UpdateConfigRequest};
use wardnet_common::auth::AuthContext;
use wardnet_common::update::{InstallPhase, Release, UpdateChannel};
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
        false,
        "0.1.0",
        tokio_util::sync::CancellationToken::new(),
    ));
    (svc, applier, events)
}

/// Build a service standing in for a daemon that has just booted on
/// `current_version`, with `config` carrying whatever markers survived the
/// restart. Hands back the config and history so tests can assert on what the
/// startup reconcile wrote.
fn build_service_at_version(
    current_version: &str,
    config: Arc<MemoryConfig>,
) -> (Arc<UpdateServiceImpl>, Arc<MemoryHistory>) {
    let history = Arc::new(MemoryHistory::default());
    let svc = Arc::new(UpdateServiceImpl::new(
        config,
        history.clone(),
        Arc::new(StubReleaseSource(None)),
        Arc::new(AlwaysOkVerifier),
        Arc::new(RecordingApplier::default()),
        Arc::new(BroadcastEventBus::new(32)),
        false,
        false,
        current_version,
        tokio_util::sync::CancellationToken::new(),
    ));
    (svc, history)
}

/// Build a service whose deploy-time `[update] allow_edge_channel` flag is
/// `allow_edge`, over a caller-supplied config so tests can seed a stored
/// channel and assert on what the startup gate wrote back.
fn build_service_gated(config: Arc<MemoryConfig>, allow_edge: bool) -> Arc<UpdateServiceImpl> {
    Arc::new(UpdateServiceImpl::new(
        config,
        Arc::new(MemoryHistory::default()),
        Arc::new(StubReleaseSource(None)),
        Arc::new(AlwaysOkVerifier),
        Arc::new(RecordingApplier::default()),
        Arc::new(BroadcastEventBus::new(32)),
        false,
        allow_edge,
        "0.1.0",
        tokio_util::sync::CancellationToken::new(),
    ))
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
async fn update_config_rejects_edge_when_flag_absent() {
    // Edge requires root on the box (a TOML flag), not merely an admin
    // session — so an authenticated admin must still be turned away.
    let config = Arc::new(MemoryConfig::default());
    let svc: Arc<dyn UpdateService> = build_service_gated(config.clone(), false);
    let result = auth_context::with_context(
        test_admin_ctx(),
        svc.update_config(UpdateConfigRequest {
            auto_update_enabled: None,
            channel: Some(UpdateChannel::Edge),
        }),
    )
    .await;
    assert!(matches!(result, Err(crate::error::AppError::Forbidden(_))));
    // Rejected, not partially applied.
    assert_eq!(config.get("update_channel").await.unwrap(), None);
}

#[tokio::test]
async fn update_config_accepts_edge_when_flag_set() {
    let config = Arc::new(MemoryConfig::default());
    let svc: Arc<dyn UpdateService> = build_service_gated(config.clone(), true);
    let resp = auth_context::with_context(
        test_admin_ctx(),
        svc.update_config(UpdateConfigRequest {
            auto_update_enabled: None,
            channel: Some(UpdateChannel::Edge),
        }),
    )
    .await
    .unwrap();
    assert_eq!(resp.status.channel, UpdateChannel::Edge);
    assert_eq!(
        config.get("update_channel").await.unwrap().as_deref(),
        Some("edge")
    );
}

#[tokio::test]
async fn status_advertises_whether_edge_is_available() {
    let gated: Arc<dyn UpdateService> =
        build_service_gated(Arc::new(MemoryConfig::default()), false);
    let resp = auth_context::with_context(test_admin_ctx(), gated.status())
        .await
        .unwrap();
    assert!(
        !resp.status.edge_available,
        "edge must not be advertised without the TOML flag"
    );

    let open: Arc<dyn UpdateService> = build_service_gated(Arc::new(MemoryConfig::default()), true);
    let resp = auth_context::with_context(test_admin_ctx(), open.status())
        .await
        .unwrap();
    assert!(resp.status.edge_available);
}

#[tokio::test]
async fn startup_gate_falls_back_to_beta_when_edge_flag_removed() {
    // A box left on edge, then redeployed without the flag: it must not keep
    // following edge, and the stored state must not contradict the config.
    let config = Arc::new(MemoryConfig::default());
    config.set("update_channel", "edge").await.unwrap();

    let svc: Arc<dyn UpdateService> = build_service_gated(config.clone(), false);
    auth_context::with_context(test_admin_ctx(), svc.reconcile_channel_gate())
        .await
        .unwrap();

    assert_eq!(
        config.get("update_channel").await.unwrap().as_deref(),
        Some("beta"),
        "fallback must be written back, not just computed at read time"
    );
    let resp = auth_context::with_context(test_admin_ctx(), svc.status())
        .await
        .unwrap();
    assert_eq!(resp.status.channel, UpdateChannel::Beta);
}

#[tokio::test]
async fn startup_gate_leaves_edge_alone_when_flag_present() {
    let config = Arc::new(MemoryConfig::default());
    config.set("update_channel", "edge").await.unwrap();

    let svc: Arc<dyn UpdateService> = build_service_gated(config.clone(), true);
    auth_context::with_context(test_admin_ctx(), svc.reconcile_channel_gate())
        .await
        .unwrap();

    assert_eq!(
        config.get("update_channel").await.unwrap().as_deref(),
        Some("edge")
    );
}

#[tokio::test]
async fn startup_gate_leaves_non_edge_channels_untouched() {
    let config = Arc::new(MemoryConfig::default());
    config.set("update_channel", "stable").await.unwrap();

    let svc: Arc<dyn UpdateService> = build_service_gated(config.clone(), false);
    auth_context::with_context(test_admin_ctx(), svc.reconcile_channel_gate())
        .await
        .unwrap();

    assert_eq!(
        config.get("update_channel").await.unwrap().as_deref(),
        Some("stable")
    );
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

// --- startup reconcile of the pending-version marker ---
//
// `run_install` persists `update_pending_version` and restarts the daemon via
// systemd. These cover what must happen on the way back up: the marker is a
// *claim* about a restart that hasn't been verified yet, so booting must
// either confirm it (and say so) or contradict it (and say that).

#[tokio::test]
async fn reconcile_clears_pending_and_records_applied_when_new_binary_is_running() {
    // Daemon restarted after installing 0.2.0 and came back up as 0.2.0.
    let config = Arc::new(MemoryConfig::default());
    config.set("update_pending_version", "0.2.0").await.unwrap();
    let (svc, _history) = build_service_at_version("0.2.0", config);

    auth_context::with_context(test_admin_ctx(), svc.reconcile_pending_install())
        .await
        .expect("reconcile ok");

    let status =
        auth_context::with_context(test_admin_ctx(), (svc as Arc<dyn UpdateService>).status())
            .await
            .unwrap()
            .status;

    // The update landed — nothing is pending any more.
    assert_eq!(
        status.pending_version, None,
        "pending marker must be cleared once the new binary is running"
    );
    // ...and the daemon records that it landed, so a polling UI can tell the
    // user. There is no event channel to the browser.
    assert_eq!(status.applied_version.as_deref(), Some("0.2.0"));
    assert!(status.applied_at.is_some(), "applied_at must be stamped");
    assert_eq!(status.install_phase, InstallPhase::Applied);
}

#[tokio::test]
async fn reconcile_marks_failed_when_restart_came_back_on_the_old_binary() {
    // Swap silently reverted: we staged 0.2.0 but rebooted into 0.1.0.
    let config = Arc::new(MemoryConfig::default());
    config.set("update_pending_version", "0.2.0").await.unwrap();
    let (svc, history) = build_service_at_version("0.1.0", config);

    auth_context::with_context(test_admin_ctx(), svc.reconcile_pending_install())
        .await
        .expect("reconcile ok");

    let status =
        auth_context::with_context(test_admin_ctx(), (svc as Arc<dyn UpdateService>).status())
            .await
            .unwrap()
            .status;

    assert_eq!(
        status.pending_version, None,
        "a pending marker that outlived its restart must not survive boot"
    );
    assert_eq!(
        status.applied_version, None,
        "nothing was applied — 0.2.0 never ran"
    );
    match status.install_phase {
        InstallPhase::Failed { ref reason } => {
            assert!(
                reason.contains("0.2.0") && reason.contains("0.1.0"),
                "reason should name both the intended and running version, got: {reason}"
            );
        }
        other => panic!("expected Failed phase, got {other:?}"),
    }

    // The reverted swap must be visible in history, not just in memory.
    let rows = history.list(10).await.unwrap();
    let row = rows
        .first()
        .expect("expected a history row for the failed swap");
    assert_eq!(row.status, UpdateHistoryStatus::Failed);
    assert_eq!(row.to_version, "0.2.0");
}

#[tokio::test]
async fn reconcile_is_a_noop_when_nothing_was_pending() {
    // Ordinary boot — no install was staged.
    let config = Arc::new(MemoryConfig::default());
    let (svc, history) = build_service_at_version("0.1.0", config);

    auth_context::with_context(test_admin_ctx(), svc.reconcile_pending_install())
        .await
        .expect("reconcile ok");

    let status =
        auth_context::with_context(test_admin_ctx(), (svc as Arc<dyn UpdateService>).status())
            .await
            .unwrap()
            .status;

    assert_eq!(status.pending_version, None);
    assert_eq!(status.applied_version, None);
    assert_eq!(status.install_phase, InstallPhase::Idle);
    assert!(
        history.list(10).await.unwrap().is_empty(),
        "a clean boot must not write history rows"
    );
}

// --- review follow-ups: the applied marker must not outlive its news ---

#[tokio::test]
async fn reconcile_requires_admin() {
    let (svc, _history) = build_service_at_version("0.2.0", Arc::new(MemoryConfig::default()));
    // No auth context: the guard must reject, like every other service method.
    let result = (svc as Arc<dyn UpdateService>)
        .reconcile_pending_install()
        .await;
    assert!(matches!(result, Err(crate::error::AppError::Forbidden(_))));
}

/// A browser opening the UI weeks later must not be told about an ancient
/// update as if it just landed: clients dedupe in local storage, so a marker
/// the daemon advertises forever becomes "news" to every new browser.
#[tokio::test]
async fn a_stale_applied_marker_is_not_advertised() {
    let config = Arc::new(MemoryConfig::default());
    config.set("update_applied_version", "0.2.0").await.unwrap();
    config
        .set(
            "update_applied_at",
            &(Utc::now() - chrono::Duration::days(21)).to_rfc3339(),
        )
        .await
        .unwrap();
    let (svc, _) = build_service_at_version("0.2.0", config);

    let status =
        auth_context::with_context(test_admin_ctx(), (svc as Arc<dyn UpdateService>).status())
            .await
            .unwrap()
            .status;

    assert_eq!(
        status.applied_version, None,
        "a three-week-old update is not news"
    );
    assert_eq!(status.applied_at, None);
}

/// ...but the update the operator just watched land IS news.
#[tokio::test]
async fn a_fresh_applied_marker_is_advertised() {
    let config = Arc::new(MemoryConfig::default());
    config.set("update_pending_version", "0.2.0").await.unwrap();
    let (svc, _) = build_service_at_version("0.2.0", config);
    let svc: Arc<dyn UpdateService> = svc;

    auth_context::with_context(test_admin_ctx(), svc.reconcile_pending_install())
        .await
        .expect("reconcile ok");
    let status = auth_context::with_context(test_admin_ctx(), svc.status())
        .await
        .unwrap()
        .status;

    assert_eq!(status.applied_version.as_deref(), Some("0.2.0"));
    assert_eq!(status.install_phase, InstallPhase::Applied);
}

/// `Applied` is a momentary phase. Nothing else returns it to `Idle`, so once
/// the announcement window closes the card must stop showing an install phase.
#[tokio::test]
async fn the_applied_phase_decays_to_idle_once_stale() {
    let config = Arc::new(MemoryConfig::default());
    config.set("update_pending_version", "0.2.0").await.unwrap();
    let (svc, _) = build_service_at_version("0.2.0", config.clone());
    let svc: Arc<dyn UpdateService> = svc;

    auth_context::with_context(test_admin_ctx(), svc.reconcile_pending_install())
        .await
        .expect("reconcile ok");

    // Rewind the stamp past the announcement window.
    config
        .set(
            "update_applied_at",
            &(Utc::now() - chrono::Duration::hours(2)).to_rfc3339(),
        )
        .await
        .unwrap();

    let status = auth_context::with_context(test_admin_ctx(), svc.status())
        .await
        .unwrap()
        .status;

    assert_eq!(
        status.install_phase,
        InstallPhase::Idle,
        "the Applied chip must not linger forever"
    );
}

/// Rolling back retracts the announcement. Otherwise a browser that had not yet
/// seen it would toast "updated to v0.2.0" right after the operator abandoned
/// v0.2.0.
#[tokio::test]
async fn rollback_retracts_the_applied_announcement() {
    let config = Arc::new(MemoryConfig::default());
    config.set("update_pending_version", "0.2.0").await.unwrap();
    let applier = Arc::new(RecordingApplier::default());
    // Give the applier something to roll back to.
    applier
        .apply(&[], &[])
        .await
        .expect("stage a previous binary");

    let svc: Arc<dyn UpdateService> = Arc::new(UpdateServiceImpl::new(
        config.clone(),
        Arc::new(MemoryHistory::default()),
        Arc::new(StubReleaseSource(None)),
        Arc::new(AlwaysOkVerifier),
        applier,
        Arc::new(BroadcastEventBus::new(32)),
        false,
        false,
        "0.2.0",
        tokio_util::sync::CancellationToken::new(),
    ));

    auth_context::with_context(test_admin_ctx(), svc.reconcile_pending_install())
        .await
        .expect("reconcile ok");
    auth_context::with_context(test_admin_ctx(), svc.rollback())
        .await
        .expect("rollback ok");

    let status = auth_context::with_context(test_admin_ctx(), svc.status())
        .await
        .unwrap()
        .status;

    assert_eq!(
        status.applied_version, None,
        "must not keep advertising the version we just rolled away from"
    );
    assert_eq!(status.applied_at, None);
}

// ── is_newer (pure version comparator) ──────────────────────────────────

use crate::update::service::is_newer;

#[test]
fn calver_padded_components() {
    // The padded canonical CalVer form — leading zeros in month / day
    // were the reason we couldn't keep `semver::Version::parse`.
    assert!(is_newer("2026.05.10", "2026.05.03"));
    assert!(is_newer("2026.06.01", "2026.05.31"));
    assert!(!is_newer("2026.05.03", "2026.05.10"));
    assert!(!is_newer("2026.05.03", "2026.05.03"));
}

#[test]
fn calver_unpadded_compares_numerically() {
    // `2026.5.3` < `2026.10.3` — the comparator parses each component
    // as `u64` so the lexically-smaller `10` correctly outranks `5`.
    assert!(is_newer("2026.10.3", "2026.5.3"));
    assert!(!is_newer("2026.5.3", "2026.10.3"));
}

#[test]
fn semver_legacy_strings_still_compare() {
    // Pre-CalVer release tooling would have used these. Still works
    // because the comparator is tuple-of-ints, not CalVer-specific.
    assert!(is_newer("0.2.0", "0.1.0"));
    assert!(!is_newer("0.1.0", "0.2.0"));
}

#[test]
fn build_metadata_stripped_base_wins() {
    // Build tags must not mask the base-version ordering.
    assert!(is_newer("2026.05.10+gabc", "2026.05.03+gdef"));
    // A pre-release on a newer base is still newer.
    assert!(is_newer("2026.05.10-beta.1", "2026.05.03"));
}

#[test]
fn pre_release_ordering() {
    // beta.2 is newer than beta.1 on the same base.
    assert!(is_newer("2026.06.00-beta.2", "2026.06.00-beta.1"));
    assert!(!is_newer("2026.06.00-beta.1", "2026.06.00-beta.2"));
    // A release outranks any pre-release of the same base.
    assert!(is_newer("2026.06.00", "2026.06.00-beta.2"));
    assert!(!is_newer("2026.06.00-beta.2", "2026.06.00"));
    // A pre-release is still newer than an older base release.
    assert!(is_newer("2026.06.00-beta.1", "2026.05.03"));
    // Same pre-release → not newer.
    assert!(!is_newer("2026.06.00-beta.1", "2026.06.00-beta.1"));
    // Label without numeric suffix (edge-case tolerance).
    assert!(is_newer("2026.06.00-rc", "2026.06.00-beta"));
}

#[test]
fn edge_ordering_is_a_one_way_ratchet() {
    // `"edge" > "beta"` lexicographically, so an edge build outranks any
    // beta of the same base — that's how an edge box keeps up with edge.
    assert!(is_newer("2026.07.00-edge.147", "2026.07.00-beta.5"));
    assert!(is_newer("2026.07.00-edge.148", "2026.07.00-edge.147"));
    // ...and the ratchet: flipping a box back to beta does NOT walk it
    // back down to a beta of the same base. This is intentional (ADR-0023)
    // — the escape hatches are `install.sh CHANNEL=beta` (no version
    // comparison) or dropping the TOML flag and waiting for the next base.
    assert!(!is_newer("2026.07.00-beta.6", "2026.07.00-edge.147"));
    // The next base CalVer clears it, by either channel.
    assert!(is_newer("2026.08.00-beta.1", "2026.07.00-edge.147"));
    // A final release still outranks an edge build of the same base.
    assert!(is_newer("2026.07.00", "2026.07.00-edge.147"));
}

#[test]
fn malformed_returns_false() {
    // A malformed candidate must never report "newer" — that would
    // let a bad manifest entry trigger an install.
    assert!(!is_newer("not-a-version", "2026.05.03"));
    assert!(!is_newer("2026.05.03", "also-bad"));
    assert!(!is_newer("", "2026.05.03"));
}