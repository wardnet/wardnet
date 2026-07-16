//! Shared helper for building a `reqwest::Client` optionally bound to a
//! network interface via `SO_BINDTODEVICE`.
//!
//! Used by every tunnel probe that needs to force traffic onto (or off of) a
//! specific interface: [`crate::tunnel_exit_probe`] and
//! [`crate::tunnel_throughput_tester`].

/// Starts a `reqwest::ClientBuilder`, bound to `interface` via
/// `SO_BINDTODEVICE` when `Some`. `None` leaves the socket unbound so it
/// egresses the default route.
#[cfg(target_os = "linux")]
pub(crate) fn interface_bound_builder(interface: Option<&str>) -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();
    match interface {
        Some(iface) => builder.interface(iface),
        None => builder,
    }
}
