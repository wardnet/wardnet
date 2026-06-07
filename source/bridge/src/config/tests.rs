use crate::config::Config;

fn test_config() -> Config {
    Config {
        listen_addr: "127.0.0.1:8080".to_string(),
        database_url: "postgres://ignored".to_string(),
        cloudflare_api_token: "token".to_string(),
        cloudflare_zone_id: "zone-id".to_string(),
        region: "us".to_string(),
        subdomain_parent: "my.us.wardnet.network".to_string(),
        sni_listen_addr: "0.0.0.0:443".to_string(),
        dot_listen_addr: "0.0.0.0:853".to_string(),
        caddy_addr: "127.0.0.1:8443".to_string(),
        bridge_hostname: "bridge.us.wardnet.network".to_string(),
    }
}

#[test]
fn from_env_reads_required_and_optional_vars() {
    // Save current values so we can restore them after the test.
    let keys = [
        "DATABASE_URL",
        "CLOUDFLARE_API_TOKEN",
        "CLOUDFLARE_ZONE_ID",
        "REGION",
        "SUBDOMAIN_PARENT",
        "BRIDGE_HOSTNAME",
        "LISTEN_ADDR",
        "SNI_LISTEN_ADDR",
        "DOT_LISTEN_ADDR",
        "CADDY_ADDR",
    ];
    let originals: Vec<_> = keys.iter().map(|k| (*k, std::env::var(k).ok())).collect();

    // SAFETY: single-threaded test binary; no concurrent env access.
    unsafe {
        std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/db");
        std::env::set_var("CLOUDFLARE_API_TOKEN", "cf-token");
        std::env::set_var("CLOUDFLARE_ZONE_ID", "cf-zone");
        std::env::set_var("REGION", "us");
        std::env::set_var("SUBDOMAIN_PARENT", "my.us.wardnet.network");
        std::env::set_var("BRIDGE_HOSTNAME", "bridge.us.wardnet.network");
        std::env::remove_var("LISTEN_ADDR");
        std::env::remove_var("SNI_LISTEN_ADDR");
        std::env::remove_var("DOT_LISTEN_ADDR");
        std::env::remove_var("CADDY_ADDR");
    }

    let cfg = Config::from_env().expect("from_env should succeed with all required vars set");

    assert_eq!(cfg.region, "us");
    assert_eq!(cfg.bridge_hostname, "bridge.us.wardnet.network");
    assert_eq!(cfg.listen_addr, "127.0.0.1:8080"); // default
    assert_eq!(cfg.sni_listen_addr, "0.0.0.0:443"); // default
    assert_eq!(cfg.dot_listen_addr, "0.0.0.0:853"); // default
    assert_eq!(cfg.caddy_addr, "127.0.0.1:8443"); // default

    // SAFETY: restoring original values; same single-threaded context.
    unsafe {
        for (key, val) in &originals {
            match val {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}

#[test]
fn install_fqdn() {
    let cfg = test_config();
    assert_eq!(
        cfg.install_fqdn("happy-einstein"),
        "happy-einstein.my.us.wardnet.network"
    );
}

#[test]
fn acme_fqdn() {
    let cfg = test_config();
    assert_eq!(
        cfg.acme_fqdn("happy-einstein"),
        "_acme-challenge.happy-einstein.my.us.wardnet.network"
    );
}

#[test]
fn eu_region_fqdns() {
    let cfg = Config {
        region: "eu".to_string(),
        subdomain_parent: "my.eu.wardnet.network".to_string(),
        ..test_config()
    };
    assert_eq!(
        cfg.install_fqdn("bold-newton"),
        "bold-newton.my.eu.wardnet.network"
    );
    assert_eq!(
        cfg.acme_fqdn("bold-newton"),
        "_acme-challenge.bold-newton.my.eu.wardnet.network"
    );
}
