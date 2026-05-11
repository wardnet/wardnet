//! Prints the generated `OpenAPI` spec to stdout as pretty JSON.
//!
//! Driven by the `make openapi` target so the static spec under
//! `source/marketing-site/public/openapi.json` stays in sync with the daemon's
//! `#[utoipa::path]` annotations. CI gates on `git diff --exit-code` over
//! that file.
//!
//! Usage: `cargo run --bin dump_openapi --quiet > openapi.json`

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = wardnetd_api::api_doc();
    let json = serde_json::to_string_pretty(&doc)?;
    println!("{json}");
    Ok(())
}
