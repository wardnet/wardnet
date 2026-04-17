use serde::{Deserialize, Serialize};

/// Parsed `[Interface]` section of a `WireGuard` config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WgInterfaceConfig {
    pub private_key: String,
    pub address: Vec<String>,
    pub listen_port: Option<u16>,
    pub dns: Vec<String>,
}

/// Parsed `[Peer]` section of a `WireGuard` config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WgPeerConfig {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    pub preshared_key: Option<String>,
    pub persistent_keepalive: Option<u16>,
}

/// Complete parsed `WireGuard` configuration file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WgConfig {
    pub interface: WgInterfaceConfig,
    pub peers: Vec<WgPeerConfig>,
}

/// Error returned when parsing a `WireGuard` config file.
#[derive(Debug, thiserror::Error)]
pub enum WgConfigError {
    #[error("missing [Interface] section")]
    MissingInterface,
    #[error("missing PrivateKey in [Interface]")]
    MissingPrivateKey,
    #[error("missing [Peer] section")]
    MissingPeer,
    #[error("missing PublicKey in [Peer]")]
    MissingPublicKey,
    #[error("invalid line: {0}")]
    InvalidLine(String),
    #[error("unknown section: {0}")]
    UnknownSection(String),
}

/// Splits a comma-separated value string into trimmed, non-empty entries.
fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Which section the parser is currently inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Interface,
    Peer,
}

/// Intermediate builder for assembling interface fields during parsing.
#[derive(Default)]
struct InterfaceBuilder {
    private_key: Option<String>,
    address: Vec<String>,
    listen_port: Option<u16>,
    dns: Vec<String>,
    present: bool,
}

/// Intermediate builder for assembling peer fields during parsing.
#[derive(Default)]
struct PeerBuilder {
    public_key: Option<String>,
    endpoint: Option<String>,
    allowed_ips: Vec<String>,
    preshared_key: Option<String>,
    persistent_keepalive: Option<u16>,
    active: bool,
}

impl PeerBuilder {
    /// Consumes the accumulated fields and produces a [`WgPeerConfig`].
    /// Returns `MissingPublicKey` if no `PublicKey` was set.
    fn flush(&mut self) -> Result<Option<WgPeerConfig>, WgConfigError> {
        if !self.active {
            return Ok(None);
        }
        let pk = self
            .public_key
            .take()
            .ok_or(WgConfigError::MissingPublicKey)?;
        let peer = WgPeerConfig {
            public_key: pk,
            endpoint: self.endpoint.take(),
            allowed_ips: std::mem::take(&mut self.allowed_ips),
            preshared_key: self.preshared_key.take(),
            persistent_keepalive: self.persistent_keepalive.take(),
        };
        self.active = false;
        Ok(Some(peer))
    }
}

/// Parses a `WireGuard` `.conf` file from its text content.
///
/// Returns a [`WgConfig`] on success or a [`WgConfigError`] describing the
/// first problem encountered.
pub fn parse(input: &str) -> Result<WgConfig, WgConfigError> {
    let mut section: Option<Section> = None;
    let mut iface = InterfaceBuilder::default();
    let mut peer = PeerBuilder::default();
    let mut peers: Vec<WgPeerConfig> = Vec::new();

    for raw_line in input.lines() {
        let line = raw_line.trim().trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            parse_section_header(line, &mut section, &mut iface, &mut peer, &mut peers)?;
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(WgConfigError::InvalidLine(line.to_owned()));
        };
        apply_key_value(key.trim(), value.trim(), section, &mut iface, &mut peer);
    }

    // Flush the last peer.
    if let Some(p) = peer.flush()? {
        peers.push(p);
    }

    validate_and_build(iface, peers)
}

/// Handles a `[SectionName]` header line.
fn parse_section_header(
    line: &str,
    section: &mut Option<Section>,
    iface: &mut InterfaceBuilder,
    peer: &mut PeerBuilder,
    peers: &mut Vec<WgPeerConfig>,
) -> Result<(), WgConfigError> {
    let header = &line[1..line.len() - 1];
    match header.to_ascii_lowercase().as_str() {
        "interface" => {
            iface.present = true;
            *section = Some(Section::Interface);
        }
        "peer" => {
            if let Some(p) = peer.flush()? {
                peers.push(p);
            }
            peer.active = true;
            *section = Some(Section::Peer);
        }
        other => return Err(WgConfigError::UnknownSection(other.to_owned())),
    }
    Ok(())
}

/// Applies a parsed key-value pair to the appropriate builder.
fn apply_key_value(
    key: &str,
    value: &str,
    section: Option<Section>,
    iface: &mut InterfaceBuilder,
    peer: &mut PeerBuilder,
) {
    let key_lower = key.to_ascii_lowercase();
    match section {
        Some(Section::Interface) => match key_lower.as_str() {
            "privatekey" => iface.private_key = Some(value.to_owned()),
            "address" => iface.address.extend(split_csv(value)),
            "listenport" => iface.listen_port = value.parse().ok(),
            "dns" => iface.dns.extend(split_csv(value)),
            _ => {}
        },
        Some(Section::Peer) => match key_lower.as_str() {
            "publickey" => peer.public_key = Some(value.to_owned()),
            "endpoint" => peer.endpoint = Some(value.to_owned()),
            "allowedips" => peer.allowed_ips.extend(split_csv(value)),
            "presharedkey" => peer.preshared_key = Some(value.to_owned()),
            "persistentkeepalive" => peer.persistent_keepalive = value.parse().ok(),
            _ => {}
        },
        None => {}
    }
}

/// Validates accumulated state and builds the final [`WgConfig`].
fn validate_and_build(
    iface: InterfaceBuilder,
    peers: Vec<WgPeerConfig>,
) -> Result<WgConfig, WgConfigError> {
    if !iface.present {
        return Err(WgConfigError::MissingInterface);
    }
    let private_key = iface.private_key.ok_or(WgConfigError::MissingPrivateKey)?;
    if peers.is_empty() {
        return Err(WgConfigError::MissingPeer);
    }
    Ok(WgConfig {
        interface: WgInterfaceConfig {
            private_key,
            address: iface.address,
            listen_port: iface.listen_port,
            dns: iface.dns,
        },
        peers,
    })
}
