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
    /// mDNS + SSDP/DLNA + Chromecast/Cast (8008/8009/8443/9000) + `AirPlay`
    /// (7000/7100) — the ports a phone needs to discover and stream to a TV
    /// across a zone boundary. 8443 is the Google Home app's TLS device-listing
    /// port; without it the app won't list discovered receivers.
    Casting,
    /// Screen/desktop mirroring and local-file casting — the SENDER is the live
    /// media source over dynamically-negotiated ports (Cast tab-mirroring
    /// UDP 32768-61000, `AirPlay` mirroring 49152-65535, VLC HTTP, etc.), so it
    /// opens all ports between the two endpoints. Device-to-device only (enforced
    /// in the service); requires the cross-zone NAT exemption so the receiver
    /// reaches the sender's real IP.
    Mirroring,
    /// Vendor LAN-control `IoT` — the ports a phone needs to *control* a
    /// smart-home appliance across a zone boundary (issue #1098). A third
    /// traffic model alongside the two above: client-initiated unicast control
    /// of an appliance, with no media stream, where the device also sends
    /// unsolicited replies of its own.
    ///
    /// Covers Govee (4001/4002/4003), Tuya / Smart Life (5683, 6668), local
    /// MQTT/TLS (8883), LIFX (56700) and `ESPHome` (9123). Deliberately
    /// **excludes TCP 80/443**: several vendors expose local HTTP control
    /// there, but a zone-scoped hole on those ports reaches every HTTP listener
    /// in the peer zone (NAS, cameras, dev servers) — far wider than the rest
    /// of the list, and not something a preset should smuggle in. Admins who
    /// need it pair this with the admin-site's separate `web` bundle,
    /// preferably device-scoped.
    ///
    /// Must be used **bidirectionally**: the device→client leg (Govee's UDP
    /// 4002) is a fresh flow, not a conntrack reply — the multicast discovery
    /// it answers never transits the Pi, so no conntrack entry exists for it.
    SmartHome,
}

/// A single-port [`PortSpec`] (`from == to`) — the shape every preset entry
/// below takes, so each port reads as one line with its rationale beside it.
const fn one(proto: Proto, port: u16) -> PortSpec {
    PortSpec {
        proto,
        from: port,
        to: port,
    }
}

impl ServiceSet {
    /// The concrete ports this preset expands to. Each port is a single-value
    /// [`PortSpec`] (`from == to`), except [`Self::Mirroring`], which spans the
    /// full range.
    #[must_use]
    pub fn ports(&self) -> Vec<PortSpec> {
        use Proto::{Tcp, Udp};
        match self {
            Self::Casting => vec![
                one(Udp, 5353), // mDNS
                one(Udp, 1900), // SSDP / DLNA
                one(Tcp, 8008), // Chromecast / Cast
                one(Tcp, 8009), // Chromecast / Cast
                one(Tcp, 8443), // Google Home app device listing (TLS)
                one(Tcp, 9000), // Chromecast / Cast
                one(Tcp, 7000), // AirPlay
                one(Tcp, 7100), // AirPlay
            ],
            // Mirroring negotiates media ports dynamically, so the exemption
            // spans the full range on both protocols between the two devices.
            Self::Mirroring => vec![
                PortSpec {
                    proto: Tcp,
                    from: 1,
                    to: u16::MAX,
                },
                PortSpec {
                    proto: Udp,
                    from: 1,
                    to: u16::MAX,
                },
            ],
            // Vendor LAN-control ports only — no 80/443, see the variant doc.
            // 5353/1900 are here for the *unicast* leg: an app querying a known
            // device IP directly. The multicast discovery leg crosses zones at
            // L2 and never transits the Pi, so no rule of ours carries it
            // (ADR 0021 §5).
            Self::SmartHome => vec![
                one(Udp, 4001),  // Govee LAN API discovery
                one(Udp, 4002),  // Govee device -> client responses
                one(Udp, 4003),  // Govee client -> device control
                one(Udp, 5353),  // mDNS (unicast queries)
                one(Udp, 1900),  // SSDP (unicast M-SEARCH)
                one(Udp, 5683),  // CoAP — Tuya / Smart Life / LocalTuya
                one(Tcp, 6668),  // Tuya local control
                one(Tcp, 8883),  // Local MQTT/TLS — Shelly gen2, ESPHome
                one(Udp, 56700), // LIFX
                one(Tcp, 9123),  // ESPHome native API
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
