use crate::config::Config;

fn test_config() -> Config {
    Config {
        http01_listen_addr: "127.0.0.1:8080".to_string(),
        tls_listen_addr: "127.0.0.1:8443".to_string(),
        dot_listen_addr: "127.0.0.1:8853".to_string(),
        database_url: "postgres://ignored".to_string(),
        global_database_url: "postgres://ignored-global".to_string(),
        cloudflare_api_token: "token".to_string(),
        cloudflare_zone_id: "zone-id".to_string(),
        region: "use1".to_string(),
        subdomain_parent: "my.wardnet.services".to_string(),
        fqdn: "bridge.svc.prod.use1.wardnet.network".to_string(),
        acme_directory_url: "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
        encryption_key: [7u8; 32],
    }
}

#[test]
fn from_env_reads_required_and_optional_vars() {
    let keys = [
        "DATABASE_URL",
        "GLOBAL_DATABASE_URL",
        "CLOUDFLARE_API_TOKEN",
        "CLOUDFLARE_ZONE_ID",
        "INFORGE_DEPLOYMENT_REGION_SLUG",
        "INFORGE_DEPLOYMENT_FQDN",
        "INFORGE_DEPLOYMENT_ENVIRONMENT",
        "SUBDOMAIN_PARENT",
        "ENCRYPTION_KEY",
        "HTTP01_LISTEN_ADDR",
        "TLS_LISTEN_ADDR",
        "DOT_LISTEN_ADDR",
        "ACME_DIRECTORY_URL",
    ];
    let originals: Vec<_> = keys.iter().map(|k| (*k, std::env::var(k).ok())).collect();

    // A 32-byte key, base64-encoded.
    let key_b64 = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode([3u8; 32])
    };

    // SAFETY: single-threaded test binary; no concurrent env access.
    unsafe {
        std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/db");
        std::env::set_var(
            "GLOBAL_DATABASE_URL",
            "postgres://test:test@localhost/global",
        );
        std::env::set_var("CLOUDFLARE_API_TOKEN", "cf-token");
        std::env::set_var("CLOUDFLARE_ZONE_ID", "cf-zone");
        std::env::set_var("INFORGE_DEPLOYMENT_REGION_SLUG", "use1");
        std::env::set_var(
            "INFORGE_DEPLOYMENT_FQDN",
            "bridge.svc.prod.use1.wardnet.network",
        );
        std::env::set_var("INFORGE_DEPLOYMENT_ENVIRONMENT", "prod");
        std::env::set_var("SUBDOMAIN_PARENT", "my.wardnet.services");
        std::env::set_var("ENCRYPTION_KEY", &key_b64);
        std::env::remove_var("HTTP01_LISTEN_ADDR");
        std::env::remove_var("TLS_LISTEN_ADDR");
        std::env::remove_var("DOT_LISTEN_ADDR");
        std::env::remove_var("ACME_DIRECTORY_URL");
    }

    let cfg = Config::from_env().expect("from_env should succeed with all required vars set");

    assert_eq!(cfg.region, "use1");
    assert_eq!(
        cfg.global_database_url,
        "postgres://test:test@localhost/global"
    );
    assert_eq!(cfg.fqdn, "bridge.svc.prod.use1.wardnet.network");
    assert_eq!(cfg.http01_listen_addr, "127.0.0.1:8080"); // default
    assert_eq!(cfg.tls_listen_addr, "127.0.0.1:8443"); // default
    assert_eq!(cfg.dot_listen_addr, "127.0.0.1:8853"); // default
    // `prod` environment ⇒ Let's Encrypt production directory.
    assert_eq!(
        cfg.acme_directory_url,
        "https://acme-v02.api.letsencrypt.org/directory"
    );
    assert_eq!(cfg.encryption_key, [3u8; 32]);

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
        "happy-einstein.my.wardnet.services"
    );
}

#[test]
fn acme_fqdn() {
    let cfg = test_config();
    assert_eq!(
        cfg.acme_fqdn("happy-einstein"),
        "_acme-challenge.happy-einstein.my.wardnet.services"
    );
}

#[test]
fn region_label_independent_of_user_fqdn() {
    // The user-facing host is region-free: an EU bridge uses the same flat
    // `my.wardnet.services` parent as US, so generated tenant FQDNs carry no region
    // label. Region lives only in `region`/`fqdn`, never the tenant host.
    let cfg = Config {
        region: "euw1".to_string(),
        subdomain_parent: "my.wardnet.services".to_string(),
        ..test_config()
    };
    assert_eq!(
        cfg.install_fqdn("bold-newton"),
        "bold-newton.my.wardnet.services"
    );
    assert_eq!(
        cfg.acme_fqdn("bold-newton"),
        "_acme-challenge.bold-newton.my.wardnet.services"
    );
}

#[test]
fn debug_redacts_secrets() {
    // The custom Debug must hide secret-bearing fields but keep the safe ones.
    let dump = format!("{:?}", test_config());
    assert!(dump.contains("<redacted>"));
    assert!(dump.contains("use1")); // region kept
    assert!(dump.contains("zone-id")); // cloudflare_zone_id is not secret
    // Secret values must not leak (field *names* may still appear).
    assert!(!dump.contains("postgres://ignored")); // database_url redacted
    assert!(!dump.contains("postgres://ignored-global")); // global_database_url redacted
}

#[test]
fn required_errors_when_absent() {
    // A uniquely-named, guaranteed-unset variable avoids racing other tests.
    let err = super::required("WARDNET_TEST_DEFINITELY_UNSET_VAR").unwrap_err();
    assert!(err.to_string().contains("is not set"));
}

#[test]
fn encryption_key_accepts_valid_base64_32() {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode([9u8; 32]);
    // SAFETY: uniquely-named var, single-threaded test binary.
    unsafe { std::env::set_var("WARDNET_TEST_ENC_OK", &b64) };
    let got = super::encryption_key("WARDNET_TEST_ENC_OK").unwrap();
    unsafe { std::env::remove_var("WARDNET_TEST_ENC_OK") };
    assert_eq!(got, [9u8; 32]);
}

#[test]
fn encryption_key_rejects_bad_base64() {
    // SAFETY: uniquely-named var, single-threaded test binary.
    unsafe { std::env::set_var("WARDNET_TEST_ENC_BAD", "!!! not base64 !!!") };
    let err = super::encryption_key("WARDNET_TEST_ENC_BAD").unwrap_err();
    unsafe { std::env::remove_var("WARDNET_TEST_ENC_BAD") };
    assert!(err.to_string().contains("not valid base64"));
}

#[test]
fn encryption_key_rejects_wrong_length() {
    use base64::Engine as _;
    // 16 bytes, not 32.
    let b64 = base64::engine::general_purpose::STANDARD.encode([1u8; 16]);
    // SAFETY: uniquely-named var, single-threaded test binary.
    unsafe { std::env::set_var("WARDNET_TEST_ENC_SHORT", &b64) };
    let err = super::encryption_key("WARDNET_TEST_ENC_SHORT").unwrap_err();
    unsafe { std::env::remove_var("WARDNET_TEST_ENC_SHORT") };
    assert!(err.to_string().contains("exactly 32 bytes"));
}
