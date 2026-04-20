//! Background-job types.
//!
//! Handlers that kick off work too slow for a synchronous HTTP response
//! dispatch a [`Job`] and return its id. The client polls `GET /api/jobs/:id`
//! until the job reaches a terminal state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Discriminator for what kind of work a job represents. The kind is fixed
/// at job creation time; new kinds get new variants here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    BlocklistRefresh,
}

/// Lifecycle state of a [`Job`]. Jobs start as `Pending`, move to `Running`
/// when execution begins, and end in one of the two terminal states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    Pending,
    Running,
    #[serde(rename = "SUCCEED")]
    Succeeded,
    #[serde(rename = "TERMINATED_WITH_ERRORS")]
    Failed,
}

impl JobStatus {
    /// A job in a terminal state stays in the registry for the GC TTL and
    /// then disappears. Pollers should stop once they observe a terminal
    /// status.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, JobStatus::Succeeded | JobStatus::Failed)
    }
}

/// Snapshot of a job, returned by `GET /api/jobs/:id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,
    pub kind: JobKind,
    pub status: JobStatus,
    /// Completion percentage, 0..=100. Jumps to 100 when the job terminates.
    pub percentage_done: u8,
    /// Populated when `status == Failed`.
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response body for a handler that dispatches a background job instead of
/// doing the work inline. The HTTP status code is `202 Accepted`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDispatchedResponse {
    pub job_id: Uuid,
}
