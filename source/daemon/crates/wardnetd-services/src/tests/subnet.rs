//! Tests for the shared IPv4-subnet helpers.

use crate::subnet::{canonical_cidr, gateway_for, pool_bounds};
use ipnetwork::Ipv4Network;
use std::net::Ipv4Addr;

#[test]
fn gateway_is_first_host() {
    let net: Ipv4Network = "10.44.0.0/24".parse().unwrap();
    assert_eq!(gateway_for(net), Ipv4Addr::new(10, 44, 0, 1));
}

#[test]
fn pool_bounds_for_slash24() {
    let net: Ipv4Network = "10.44.0.0/24".parse().unwrap();
    assert_eq!(
        pool_bounds(net),
        Some((Ipv4Addr::new(10, 44, 0, 10), Ipv4Addr::new(10, 44, 0, 249)))
    );
}

#[test]
fn pool_bounds_none_for_slash30() {
    let net: Ipv4Network = "10.44.0.0/30".parse().unwrap();
    assert_eq!(pool_bounds(net), None);
}

#[test]
fn canonical_cidr_clears_host_bits() {
    let net: Ipv4Network = "10.44.1.5/24".parse().unwrap();
    assert_eq!(canonical_cidr(net), "10.44.1.0/24");
}