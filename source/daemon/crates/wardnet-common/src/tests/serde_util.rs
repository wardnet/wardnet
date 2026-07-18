#![allow(clippy::option_option)]

use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct Wrapper {
    #[serde(default, deserialize_with = "crate::serde_util::nullable_field")]
    value: Option<Option<String>>,
}

fn parse(json: &str) -> Option<Option<String>> {
    serde_json::from_str::<Wrapper>(json).unwrap().value
}

/// Field absent → `None` (leave unchanged).
#[test]
fn absent_yields_none() {
    assert_eq!(parse("{}"), None);
}

/// Field present as JSON `null` → `Some(None)` (clear to NULL).
#[test]
fn null_yields_some_none() {
    assert_eq!(parse(r#"{"value": null}"#), Some(None));
}

/// Field present as a string → `Some(Some(s))` (replace value).
#[test]
fn string_yields_some_some() {
    assert_eq!(
        parse(r#"{"value": "hello"}"#),
        Some(Some("hello".to_owned()))
    );
}
