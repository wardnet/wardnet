use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::broadcast;

use crate::error::AppError;

use super::component::{BoxedLayer, LogComponent};
use super::error_notifier::{ErrorEntry, ErrorNotifier};
use super::stream::{LogEntry, LogStream};

/// Metadata for a single log file.
#[derive(Debug, Clone, Serialize)]
pub struct LogFileInfo {
    /// File name (e.g. `wardnetd.log.2026-04-12`).
    pub name: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Last modification time.
    pub modified_at: DateTime<Utc>,
    /// Whether this is the currently active log file.
    pub active: bool,
}

/// Unified log service: orchestrates log components, exposes WebSocket
/// streaming, recent errors, and log file management.
#[async_trait]
pub trait LogService: Send + Sync {
    /// Subscribe to receive live log entries over WebSocket.
    fn subscribe(&self) -> broadcast::Receiver<LogEntry>;

    /// Return the most recent errors and warnings.
    fn get_recent_errors(&self) -> Vec<ErrorEntry>;

    /// List all available log files with metadata.
    async fn list_log_files(&self) -> Result<Vec<LogFileInfo>, AppError>;

    /// Read and format a specific log file for download.
    /// If `name` is `None`, returns the most recent (active) log file.
    async fn download_log_file(&self, name: Option<&str>) -> Result<String, AppError>;

    /// Collect tracing layers from all log components.
    ///
    /// Called once during startup to compose layers into the subscriber.
    fn tracing_layers(&self) -> Vec<BoxedLayer>;

    /// Start all log components (begin capturing events).
    fn start_all(&self);

    /// Stop all log components.
    fn stop_all(&self);
}

/// Production implementation that orchestrates [`LogStream`] and
/// [`ErrorNotifier`] components.
pub struct LogServiceImpl {
    stream: Arc<dyn LogStream>,
    stream_component: Arc<dyn LogComponent>,
    error_notifier: Arc<dyn ErrorNotifier>,
    error_component: Arc<dyn LogComponent>,
    log_path: PathBuf,
}

impl LogServiceImpl {
    /// Create a new log service.
    ///
    /// `stream` and `error_notifier` must also implement [`LogComponent`].
    /// The concrete types ([`LogStreamService`](super::stream::LogStreamService)
    /// and [`ErrorNotifierService`](super::error_notifier::ErrorNotifierService))
    /// satisfy both.
    pub fn new<S, E>(stream: Arc<S>, error_notifier: Arc<E>, log_path: PathBuf) -> Self
    where
        S: LogStream + LogComponent + 'static,
        E: ErrorNotifier + LogComponent + 'static,
    {
        Self {
            stream: stream.clone() as Arc<dyn LogStream>,
            stream_component: stream as Arc<dyn LogComponent>,
            error_notifier: error_notifier.clone() as Arc<dyn ErrorNotifier>,
            error_component: error_notifier as Arc<dyn LogComponent>,
            log_path,
        }
    }
}

#[async_trait]
impl LogService for LogServiceImpl {
    fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.stream.subscribe()
    }

    fn get_recent_errors(&self) -> Vec<ErrorEntry> {
        self.error_notifier.get_recent_errors()
    }

    async fn list_log_files(&self) -> Result<Vec<LogFileInfo>, AppError> {
        let candidates = discover_log_files(&self.log_path).await?;
        let active_path = find_active_log_path(&self.log_path, &candidates);

        let mut files = Vec::with_capacity(candidates.len());
        for path in &candidates {
            let meta = tokio::fs::metadata(path).await.map_err(|e| {
                AppError::Internal(anyhow::anyhow!(
                    "failed to stat log file {}: {e}",
                    path.display()
                ))
            })?;

            let modified_at: DateTime<Utc> =
                meta.modified().map_or_else(|_| Utc::now(), DateTime::from);

            let name = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("unknown")
                .to_owned();

            files.push(LogFileInfo {
                name,
                size_bytes: meta.len(),
                modified_at,
                active: active_path.as_deref() == Some(path.as_path()),
            });
        }

        // Most recent first.
        files.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

        Ok(files)
    }

    async fn download_log_file(&self, name: Option<&str>) -> Result<String, AppError> {
        let target_path = if let Some(name) = name {
            // Resolve within the log directory — prevent path traversal.
            let dir = self
                .log_path
                .parent()
                .ok_or_else(|| AppError::Internal(anyhow::anyhow!("invalid log path")))?;
            let resolved = dir.join(name);
            if resolved.parent() != Some(dir) {
                return Err(AppError::BadRequest("invalid log file name".to_owned()));
            }
            resolved
        } else {
            let candidates = discover_log_files(&self.log_path).await?;
            find_active_log_path(&self.log_path, &candidates).ok_or_else(|| {
                AppError::NotFound(format!("no log files found at {}", self.log_path.display()))
            })?
        };

        let content = tokio::fs::read_to_string(&target_path).await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "failed to read log file {}: {e}",
                target_path.display()
            ))
        })?;

        let formatted: String = content
            .lines()
            .map(format_log_line)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(formatted)
    }

    fn tracing_layers(&self) -> Vec<BoxedLayer> {
        vec![
            self.stream_component.tracing_layer(),
            self.error_component.tracing_layer(),
        ]
    }

    fn start_all(&self) {
        self.stream_component.start();
        self.error_component.start();
    }

    fn stop_all(&self) {
        self.stream_component.stop();
        self.error_component.stop();
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Discover all log files matching the configured path prefix.
async fn discover_log_files(configured_path: &Path) -> Result<Vec<PathBuf>, AppError> {
    let dir = configured_path
        .parent()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("invalid log path")))?;
    let prefix = configured_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("wardnetd.log");

    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to read log directory: {e}")))?;

    let mut candidates = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Some(name) = entry.file_name().to_str()
            && name.starts_with(prefix)
        {
            candidates.push(entry.path());
        }
    }

    candidates.sort();
    Ok(candidates)
}

/// Find the active (most recent) log file path.
fn find_active_log_path(configured_path: &Path, candidates: &[PathBuf]) -> Option<PathBuf> {
    if candidates.iter().any(|p| p == configured_path) {
        return Some(configured_path.to_owned());
    }
    candidates.last().cloned()
}

/// Format a JSON log line into human-readable text.
fn format_log_line(line: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return line.to_string();
    };
    let obj = v.as_object();
    let timestamp = obj
        .and_then(|o| o.get("timestamp"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let level = obj
        .and_then(|o| o.get("level"))
        .and_then(|l| l.as_str())
        .unwrap_or("INFO");
    let target = obj
        .and_then(|o| o.get("target"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let message = obj
        .and_then(|o| o.get("fields"))
        .and_then(|f| f.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");

    format!("{timestamp} {level:>5} {target} {message}")
}
