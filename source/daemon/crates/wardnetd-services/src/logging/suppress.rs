use std::sync::Arc;

/// Prefix-matched deny-list of tracing targets that must not reach the
/// admin-facing log surfaces.
///
/// This exists because the daemon has two classes of log consumer with very
/// different needs:
///
/// * The **log file** and the `OTel` exporters want everything the subscriber's
///   `EnvFilter` lets through — that is what you go read when something broke.
/// * The **admin UI** (the WebSocket log stream and the recent-errors buffer)
///   wants only events an admin can act on. A dependency that warns once per
///   DNS query is not actionable; it drowns the live view and evicts real
///   errors out of the fixed-size recent-errors ring buffer.
///
/// Filtering with a subscriber-level `EnvFilter` directive cannot express that
/// split — it would drop the events for the file too. So the two UI-facing
/// layers consult this deny-list themselves, leaving every other sink intact.
///
/// Prefix matching means `hickory_resolver::recursor` covers the whole module
/// subtree, including `hickory_resolver::recursor::handle`. The list is
/// configured by `logging.ui_suppressed_targets`.
#[derive(Debug, Clone, Default)]
pub struct TargetSuppressor {
    prefixes: Arc<[String]>,
}

impl TargetSuppressor {
    /// Build a suppressor from a list of target prefixes.
    ///
    /// Empty entries are discarded: an empty prefix matches every target and
    /// would silence the admin UI entirely, which is never what a config
    /// typo means to say.
    #[must_use]
    pub fn new(prefixes: Vec<String>) -> Self {
        let prefixes: Vec<String> = prefixes.into_iter().filter(|p| !p.is_empty()).collect();
        Self {
            prefixes: prefixes.into(),
        }
    }

    /// Whether an event from `target` at `level` should be hidden from the
    /// admin UI.
    ///
    /// ERROR is **never** suppressed, whatever the target. The point of this
    /// list is per-event chatter — a dependency that warns once per DNS query.
    /// A dependency that logs an ERROR is saying something went genuinely
    /// wrong, and silencing that by module name would turn a noise filter into
    /// a blind spot: the admin would have no signal at all, in the live stream
    /// or the recent-errors buffer.
    #[must_use]
    pub fn is_suppressed(&self, target: &str, level: &tracing::Level) -> bool {
        if *level == tracing::Level::ERROR {
            return false;
        }
        self.prefixes.iter().any(|p| target.starts_with(p.as_str()))
    }
}
