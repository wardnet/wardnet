//! One module per command group. Each `run` builds the request from parsed
//! arguments, calls the [`Client`](crate::client::Client), and renders the
//! result (human or, under `--json`, verbatim).

pub mod backup;
pub mod devices;
pub mod status;
pub mod tunnels;
pub mod update;

use uuid::Uuid;
use wardnet_common::routing::RoutingTarget;

use crate::error::CliError;

/// Parse an ID argument as a UUID, with a message naming what kind of ID it is.
///
/// # Errors
/// [`CliError::Usage`] when `value` is not a valid UUID.
pub fn parse_uuid(kind: &str, value: &str) -> Result<Uuid, CliError> {
    value
        .parse::<Uuid>()
        .map_err(|_| CliError::Usage(format!("invalid {kind} ID '{value}': expected a UUID")))
}

/// Parse a `devices set-rule` target: `direct`, `default`, or a tunnel UUID.
///
/// # Errors
/// [`CliError::Usage`] when `value` is none of those.
pub fn parse_routing_target(value: &str) -> Result<RoutingTarget, CliError> {
    match value {
        "direct" => Ok(RoutingTarget::Direct),
        "default" => Ok(RoutingTarget::Default),
        other => other
            .parse::<Uuid>()
            .map(|tunnel_id| RoutingTarget::Tunnel { tunnel_id })
            .map_err(|_| {
                CliError::Usage(format!(
                    "invalid routing target '{other}': expected 'direct', 'default', \
                     or a tunnel UUID"
                ))
            }),
    }
}
