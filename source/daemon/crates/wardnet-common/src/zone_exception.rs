use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The kind of endpoint a cross-zone exception references. An endpoint is either
/// a single device or a whole zone; the `id` is interpreted against the matching
/// catalog (`devices` or `network_zones`). FKs are soft/kind-tagged and are
/// validated in the service, not the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExceptionEndpointKind {
    Device,
    Zone,
}

/// One side of a cross-zone exception: a `kind`-tagged reference to a device or
/// a zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExceptionEndpoint {
    pub kind: ExceptionEndpointKind,
    pub id: Uuid,
}

/// Transport protocol a [`PortSpec`] applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Proto {
    Tcp,
    Udp,
}

impl Proto {
    /// The lowercase wire string (`"tcp"` / `"udp"`), matching the serde
    /// representation. Used when rendering the transport-protocol match of a
    /// firewall allow rule (issue #737).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        }
    }
}

/// An inclusive port range for a single protocol. A single port is expressed as
/// `from == to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PortSpec {
    pub proto: Proto,
    pub from: u16,
    pub to: u16,
}

/// A named, curated bundle of ports for a common cross-zone use case. Presets
/// keep the wire format compact and let the daemon evolve the underlying ports
/// without a data migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceSet {
    /// mDNS + SSDP/DLNA + Chromecast/Cast + `AirPlay` — the ports a phone needs
    /// to discover and stream to a TV across a zone boundary.
    Casting,
}

impl ServiceSet {
    /// The concrete ports this preset expands to. Each port is a single-value
    /// [`PortSpec`] (`from == to`).
    #[must_use]
    pub fn ports(&self) -> Vec<PortSpec> {
        match self {
            Self::Casting => vec![
                PortSpec {
                    proto: Proto::Udp,
                    from: 5353,
                    to: 5353,
                }, // mDNS
                PortSpec {
                    proto: Proto::Udp,
                    from: 1900,
                    to: 1900,
                }, // SSDP / DLNA
                PortSpec {
                    proto: Proto::Tcp,
                    from: 8008,
                    to: 8008,
                }, // Chromecast / Cast
                PortSpec {
                    proto: Proto::Tcp,
                    from: 8009,
                    to: 8009,
                }, // Chromecast / Cast
                PortSpec {
                    proto: Proto::Tcp,
                    from: 9000,
                    to: 9000,
                }, // Chromecast / Cast
                PortSpec {
                    proto: Proto::Tcp,
                    from: 7000,
                    to: 7000,
                }, // AirPlay
                PortSpec {
                    proto: Proto::Tcp,
                    from: 7100,
                    to: 7100,
                }, // AirPlay
            ],
        }
    }
}

/// How the allowed ports of an exception are specified: either a curated preset
/// or an explicit list. Internally tagged on the wire so the two shapes are
/// distinguishable without a discriminant field colliding with payload keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServiceSpec {
    /// A named preset (e.g. `casting`).
    Preset { set: ServiceSet },
    /// An explicit list of port specs.
    Ports { ports: Vec<PortSpec> },
}

impl ServiceSpec {
    /// The concrete ports this spec resolves to — a preset expands to its
    /// curated set; an explicit list is returned as-is.
    #[must_use]
    pub fn resolve_ports(&self) -> Vec<PortSpec> {
        match self {
            Self::Preset { set } => set.ports(),
            Self::Ports { ports } => ports.clone(),
        }
    }
}

/// A cross-zone exception: an admin-granted allowance for one endpoint to reach
/// another across an otherwise-isolated zone boundary (e.g. a phone casting to a
/// TV). This is data-model only in issue #737 commit 1; the zone enforcer
/// consumes it in a later commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ZoneException {
    pub id: Uuid,
    pub from: ExceptionEndpoint,
    pub to: ExceptionEndpoint,
    pub service: ServiceSpec,
    /// Whether the allowance applies in both directions (`from ↔ to`) or only
    /// `from → to`.
    pub bidirectional: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
