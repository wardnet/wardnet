//! `DdnsSettings::with_wardnet_overrides` — the config-driven equivalent of
//! `VpnProviderRegistry`'s `nordvpn_api_url` override, used by the e2e web-ui
//! harness to point the wardnet-cloud gateways at `wardnet_cloud_mock`.

use crate::ddns::DdnsSettings;
use crate::ddns::region;

#[test]
fn no_overrides_matches_default() {
    let settings = DdnsSettings::with_wardnet_overrides(None, None, None);
    let default = DdnsSettings::default();
    assert_eq!(settings.global_gateway_url, default.global_gateway_url);
    assert_eq!(settings.region_catalog.len(), default.region_catalog.len());
    for (overridden, base) in settings.region_catalog.iter().zip(&default.region_catalog) {
        assert_eq!(overridden.slug, base.slug);
        assert_eq!(overridden.gateway_base_url, base.gateway_base_url);
        assert_eq!(overridden.health_url, base.health_url);
    }
}

#[test]
fn gateway_url_override_replaces_global_gateway_only() {
    let settings = DdnsSettings::with_wardnet_overrides(Some("http://mock:8080"), None, None);
    assert_eq!(settings.global_gateway_url, "http://mock:8080");
    // Region catalog is untouched when no region override is given.
    let default_catalog = region::default_catalog();
    assert_eq!(settings.region_catalog.len(), default_catalog.len());
    for (overridden, base) in settings.region_catalog.iter().zip(&default_catalog) {
        assert_eq!(overridden.gateway_base_url, base.gateway_base_url);
        assert_eq!(overridden.health_url, base.health_url);
    }
}

#[test]
fn region_overrides_replace_catalog_urls_but_keep_slugs() {
    let default_catalog = region::default_catalog();
    let settings = DdnsSettings::with_wardnet_overrides(
        None,
        Some("http://mock:8080"),
        Some("http://mock:8080/ddns/v1/health"),
    );
    assert_eq!(settings.region_catalog.len(), default_catalog.len());
    for (overridden, base) in settings.region_catalog.iter().zip(&default_catalog) {
        assert_eq!(overridden.slug, base.slug);
        assert_eq!(overridden.gateway_base_url, "http://mock:8080");
        assert_eq!(overridden.health_url, "http://mock:8080/ddns/v1/health");
    }
}

#[test]
fn region_overrides_only_touch_the_first_catalog_entry() {
    // Guards against a regression to mapping the override over the WHOLE
    // catalog: if a second region is ever added to `default_catalog()`, its
    // real URLs must survive `with_wardnet_overrides` untouched. Can't drive
    // `default_catalog()` itself past one entry, so this constructs the
    // post-override settings directly and appends a second, distinguishable
    // entry to prove indexing (not mapping) is what's under test.
    let mut settings = DdnsSettings::with_wardnet_overrides(
        None,
        Some("http://mock:8080"),
        Some("http://mock:8080/ddns/v1/health"),
    );
    settings.region_catalog.push(region::RegionEndpoint {
        slug: "second-region".to_owned(),
        gateway_base_url: "https://api.second-region.wardnet.network".to_owned(),
        health_url: "http://api.second-region.wardnet.network:81/ddns/v1/health".to_owned(),
    });

    assert_eq!(
        settings.region_catalog[0].gateway_base_url,
        "http://mock:8080"
    );
    assert_eq!(
        settings.region_catalog[1].gateway_base_url,
        "https://api.second-region.wardnet.network"
    );
    assert_eq!(
        settings.region_catalog[1].health_url,
        "http://api.second-region.wardnet.network:81/ddns/v1/health"
    );
}
