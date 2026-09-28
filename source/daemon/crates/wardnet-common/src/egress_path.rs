//! Egress paths: the concrete ways packets leave the box.
//!
//! An **egress path** is one tunnel interface, or `direct` (the WAN). It is
//! deliberately not the same thing as a **routing target**, which is a *policy
//! choice* and may also be `default` — a deferral that resolves to whichever
//! path the gateway policy currently selects.
//!
//! The distinction is load-bearing for diagnostics: an egress path is the thing
//! a socket can be bound to, and therefore the thing a probe can measure.
//! Probing `default` would silently change meaning the moment the policy
//! changed. See `CONTEXT.md` and ADR 0038.

use serde::{Deserialize, Serialize};

/// Which stage of a path probe an outcome describes.
///
/// The two stages exist because a bare TCP connect cannot see the failure this
/// subsystem was built for. A three-way handshake is three small packets, so an
/// MTU/MSS asymmetry passes it while every real connection stalls on the first
/// full-size segment — a connect-only probe would report the path healthy right
/// through the outage it exists to explain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStage {
    /// TCP handshake only — small packets.
    Connect,
    /// TLS handshake plus a small request, which puts full-MTU segments on the
    /// wire in both directions.
    Transfer,
}

impl ProbeStage {
    /// Stable identifier, used as a stats label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Transfer => "transfer",
        }
    }
}

/// What one stage of a probe did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageOutcome {
    pub ok: bool,
    /// Wall time for the stage, when it completed.
    pub latency_ms: Option<u64>,
    /// Why it failed, for the anomaly message.
    pub error: Option<String>,
}

impl StageOutcome {
    #[must_use]
    pub const fn succeeded(latency_ms: u64) -> Self {
        Self {
            ok: true,
            latency_ms: Some(latency_ms),
            error: None,
        }
    }

    #[must_use]
    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            latency_ms: None,
            error: Some(error.into()),
        }
    }
}

/// The result of probing one egress path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathProbeOutcome {
    pub connect: StageOutcome,
    /// `None` when `connect` failed — there was nothing to transfer over.
    pub transfer: Option<StageOutcome>,
}

impl PathProbeOutcome {
    /// The path carries no traffic at all.
    #[must_use]
    pub const fn connect_failed(&self) -> bool {
        !self.connect.ok
    }

    /// The connection establishes and then stalls.
    ///
    /// This is the MSS/MTU signature: small packets fit, full-size segments do
    /// not. It is the state that motivated the whole subsystem and the one a
    /// connect-only probe reports as healthy.
    #[must_use]
    pub fn degraded(&self) -> bool {
        self.connect.ok && self.transfer.as_ref().is_some_and(|t| !t.ok)
    }
}

/// A probed path and what came back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathHealth {
    /// The tunnel's id, or [`DIRECT_PATH`] for the WAN.
    pub path: String,
    /// Human-readable label for anomaly messages.
    pub label: String,
    pub outcome: PathProbeOutcome,
}

/// The subject id used for the direct/WAN path.
///
/// A literal rather than an id because the WAN is not a row anywhere; the
/// anomaly subject has to be stable across restarts all the same.
pub const DIRECT_PATH: &str = "direct";
