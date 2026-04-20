//! Generic background-job executor.
//!
//! Handlers that kick off work too slow for a synchronous HTTP response
//! dispatch a closure through [`JobService`]. The service records a [`Job`]
//! in an in-memory registry, spawns the closure on the tokio runtime, and
//! returns a job id. Callers poll `GET /api/jobs/:id` for progress and
//! completion.
//!
//! Terminated jobs stay in the registry for a TTL before being garbage
//! collected so late pollers still see their outcome.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use wardnet_common::jobs::{Job, JobKind, JobStatus};

/// How long a terminated job stays in the registry before GC removes it.
const JOB_GC_TTL: Duration = Duration::from_secs(30 * 60);
/// How often the GC task sweeps expired jobs.
const JOB_GC_INTERVAL: Duration = Duration::from_secs(60);

/// Type-erased job closure: takes a [`ProgressReporter`] and returns a future
/// resolving to the job result. Callers should prefer [`JobServiceExt::dispatch`]
/// which boxes this automatically.
pub type BoxedJobTask = Box<
    dyn FnOnce(ProgressReporter) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> + Send,
>;

/// Registry handle. Using a type alias keeps the long generic out of public
/// signatures in the rest of the service code.
type JobRegistry = Arc<RwLock<HashMap<Uuid, Job>>>;

/// Abstract job executor. Implementations record jobs, spawn the closure,
/// and expose a lookup by id for pollers.
#[async_trait]
pub trait JobService: Send + Sync {
    /// Register a new job with the given kind, spawn the task, and return
    /// its id. The job starts as `Pending` and transitions to `Running`
    /// before the closure begins. Callers should prefer the `dispatch`
    /// helper in [`JobServiceExt`] over calling this directly.
    async fn dispatch_boxed(&self, kind: JobKind, task: BoxedJobTask) -> Uuid;

    /// Look up a job by id. Returns `None` if the id is unknown (either
    /// never dispatched, or GC'd after its TTL elapsed).
    async fn get(&self, id: Uuid) -> Option<Job>;
}

/// Ergonomic helper so handlers can pass a naked async closure:
/// `job_service.dispatch(JobKind::Foo, |reporter| async move { ... })`.
#[async_trait]
pub trait JobServiceExt: JobService {
    async fn dispatch<F, Fut>(&self, kind: JobKind, task: F) -> Uuid
    where
        F: FnOnce(ProgressReporter) -> Fut + Send + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        self.dispatch_boxed(kind, Box::new(move |r| Box::pin(task(r))))
            .await
    }
}

impl<T: JobService + ?Sized> JobServiceExt for T {}

/// Handle passed into a job closure so it can report incremental progress.
///
/// Progress updates are best-effort: a slow update won't block the job's
/// main work since they only touch an in-memory map, but callers should
/// still use a modest update cadence (e.g. every 1–2% during a download)
/// to keep contention low.
#[derive(Clone)]
pub struct ProgressReporter {
    job_id: Uuid,
    registry: JobRegistry,
}

impl ProgressReporter {
    /// Update the job's completion percentage (clamped to `0..=100`).
    pub async fn set_progress(&self, percentage: u8) {
        let pct = percentage.min(100);
        if let Some(job) = self.registry.write().await.get_mut(&self.job_id) {
            if job.percentage_done != pct {
                job.percentage_done = pct;
                job.updated_at = Utc::now();
            }
        }
    }
}

/// Default [`JobService`] implementation backed by an in-memory `HashMap`.
pub struct JobServiceImpl {
    registry: JobRegistry,
}

impl JobServiceImpl {
    /// Construct a new service and spawn its GC task.
    pub fn new() -> Arc<Self> {
        let svc = Arc::new(Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
        });
        svc.spawn_gc();
        svc
    }

    fn spawn_gc(self: &Arc<Self>) {
        let registry = self.registry.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(JOB_GC_INTERVAL);
            interval.tick().await; // first tick fires immediately, skip it
            loop {
                interval.tick().await;
                let cutoff = Utc::now()
                    - chrono::Duration::from_std(JOB_GC_TTL)
                        .expect("JOB_GC_TTL fits in chrono::Duration");
                let mut reg = registry.write().await;
                let before = reg.len();
                reg.retain(|_, job| !(job.status.is_terminal() && job.updated_at < cutoff));
                let removed = before - reg.len();
                if removed > 0 {
                    tracing::debug!(removed, "job registry GC swept terminated jobs");
                }
            }
        });
    }
}

#[async_trait]
impl JobService for JobServiceImpl {
    async fn dispatch_boxed(&self, kind: JobKind, task: BoxedJobTask) -> Uuid {
        let job_id = Uuid::new_v4();
        let now = Utc::now();
        let job = Job {
            id: job_id,
            kind,
            status: JobStatus::Pending,
            percentage_done: 0,
            error: None,
            created_at: now,
            updated_at: now,
        };

        // Insert synchronously before spawning so that a client that polls
        // immediately after the dispatch response lands sees the job.
        self.registry.write().await.insert(job_id, job);

        let registry = self.registry.clone();
        tokio::spawn(async move {
            // Mark as running.
            if let Some(job) = registry.write().await.get_mut(&job_id) {
                job.status = JobStatus::Running;
                job.updated_at = Utc::now();
            }

            let reporter = ProgressReporter {
                job_id,
                registry: registry.clone(),
            };
            let result = task(reporter).await;

            // Record terminal state. Percentage jumps to 100 on success so
            // a UI progress bar always finishes cleanly.
            let mut reg = registry.write().await;
            if let Some(job) = reg.get_mut(&job_id) {
                match result {
                    Ok(()) => {
                        job.status = JobStatus::Succeeded;
                        job.percentage_done = 100;
                    }
                    Err(e) => {
                        job.status = JobStatus::Failed;
                        job.error = Some(e.to_string());
                    }
                }
                job.updated_at = Utc::now();
            }
        });

        job_id
    }

    async fn get(&self, id: Uuid) -> Option<Job> {
        self.registry.read().await.get(&id).cloned()
    }
}
