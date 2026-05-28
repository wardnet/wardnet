use crate::config::Config;

fn test_config() -> Config {
    Config {
        listen_addr: "127.0.0.1:8080".to_string(),
        database_url: "mysql://ignored".to_string(),
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
