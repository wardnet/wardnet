use crate::config::{Config, DEFAULT_DAEMON_URL, RawConfig};
use crate::error::CliError;

#[test]
fn parses_daemon_url_and_token() {
    let raw: RawConfig =
        toml::from_str("daemon_url = \"http://host:9000\"\ntoken = \"abc\"").unwrap();
    assert_eq!(raw.daemon_url.as_deref(), Some("http://host:9000"));
    assert_eq!(raw.token.as_deref(), Some("abc"));
}

#[test]
fn empty_config_is_valid() {
    let raw: RawConfig = toml::from_str("").unwrap();
    assert!(raw.daemon_url.is_none());
    assert!(raw.token.is_none());
}

#[test]
fn missing_token_yields_config_error() {
    let cfg = Config {
        daemon_url: DEFAULT_DAEMON_URL.to_owned(),
        token: None,
    };
    assert!(matches!(cfg.token(), Err(CliError::Config(_))));
}

#[test]
fn token_is_returned_when_present() {
    let cfg = Config {
        daemon_url: DEFAULT_DAEMON_URL.to_owned(),
        token: Some("secret".to_owned()),
    };
    assert_eq!(cfg.token().unwrap(), "secret");
}
