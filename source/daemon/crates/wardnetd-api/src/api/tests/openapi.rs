//! Tests for the generated `OpenAPI` document and the docs endpoints.
//!
//! Split into two groups:
//! 1. Shape assertions on the `api_doc()` value (title, security schemes,
//!    tag list, a spot-check of paths, and the empty-security marker on
//!    unauthenticated endpoints).
//! 2. Router-level checks that `/api/openapi.json`, `/api/docs`, and
//!    `/api/docs/logo.svg` are registered and admin-gated.
//!
//! The shape assertions are the important ones — they protect against
//! silent drift in the annotations (e.g. someone adds a new endpoint but
//! forgets the `#[utoipa::path]`, or the security scheme name is
//! misspelled somewhere).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use utoipa::openapi::security::SecurityScheme;

use crate::tests::stubs::test_app_state;

// ---------------------------------------------------------------------------
// api_doc() shape
// ---------------------------------------------------------------------------

#[test]
fn api_doc_has_title_and_license() {
    let doc = crate::api_doc();
    assert_eq!(doc.info.title, "Wardnet API");
    assert!(
        doc.info.license.is_some(),
        "license block should be populated"
    );
}

#[test]
fn api_doc_version_matches_release_version() {
    // The spec's `info.version` is pinned to wardnetd-services' compile-
    // time `RELEASE_VERSION` const (read from the workspace-root CALVER
    // file). If this fails after bumping CALVER, the wiring in
    // `openapi.rs` has drifted from `wardnetd_services::version::RELEASE_VERSION`.
    let doc = crate::api_doc();
    assert_eq!(
        doc.info.version,
        wardnetd_services::version::RELEASE_VERSION
    );
}

#[test]
fn api_doc_registers_both_security_schemes() {
    let doc = crate::api_doc();
    let components = doc.components.expect("components block must exist");
    let schemes = &components.security_schemes;

    let session = schemes
        .get("session_cookie")
        .expect("session_cookie scheme must be registered");
    assert!(
        matches!(session, SecurityScheme::ApiKey(_)),
        "session_cookie should be an ApiKey scheme"
    );

    let bearer = schemes
        .get("bearer_auth")
        .expect("bearer_auth scheme must be registered");
    assert!(
        matches!(bearer, SecurityScheme::Http(_)),
        "bearer_auth should be an Http scheme"
    );
}

#[test]
fn api_doc_tags_cover_every_handler_group() {
    let doc = crate::api_doc();
    let tag_names: Vec<_> = doc
        .tags
        .unwrap_or_default()
        .into_iter()
        .map(|t| t.name)
        .collect();

    // Matches the file list under `src/api/` — every handler module owns a
    // tag, and every tag declared here ships at least one endpoint.
    for expected in [
        "auth",
        "setup",
        "info",
        "devices",
        "tunnels",
        "providers",
        "dhcp",
        "dns",
        "system",
        "jobs",
        "update",
    ] {
        assert!(
            tag_names.iter().any(|n| n == expected),
            "missing tag: {expected} (have: {tag_names:?})"
        );
    }
}

#[test]
fn api_doc_contains_one_path_from_every_handler_module() {
    let doc = crate::api_doc();
    // One representative path per file under `src/api/` — if any module's
    // handlers silently drop off the router or lose their `#[utoipa::path]`
    // annotation, this test fails.
    for path in [
        "/api/auth/login",
        "/api/setup/status",
        "/api/info",
        "/api/devices",
        "/api/tunnels",
        "/api/providers",
        "/api/dhcp/config",
        "/api/dns/config",
        "/api/dns/filter/profiles",
        "/api/system/status",
        "/api/jobs/{id}",
        "/api/update/status",
    ] {
        assert!(
            doc.paths.paths.contains_key(path),
            "generated spec is missing expected path: {path}"
        );
    }
}

/// Serialize the spec once and return the parsed JSON value. Going through
/// serde is cheaper than figuring out which utoipa types expose accessors
/// and which are opaque — the JSON shape is the actual public contract anyway.
fn api_doc_json() -> serde_json::Value {
    serde_json::to_value(crate::api_doc()).expect("OpenApi is serializable")
}

#[test]
fn api_doc_unauthenticated_endpoint_has_empty_security() {
    // /api/info is advertised as unauthenticated via `security(())` in its
    // annotation. In OpenAPI 3 JSON that renders as a single-entry list
    // whose only entry is `{}` — meaning "no scheme required".
    let doc = api_doc_json();
    let security = &doc["paths"]["/api/info"]["get"]["security"];
    assert!(
        security.is_array(),
        "security must be an array, got {security}"
    );
    let arr = security.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected exactly one requirement entry");
    let first = arr[0].as_object().expect("entry must be an object");
    assert!(
        first.is_empty(),
        "unauthenticated endpoint should have an empty security requirement, got {first:?}"
    );
}

#[test]
fn api_doc_authenticated_endpoint_references_both_schemes() {
    // /api/devices is admin-gated; it should list both accepted schemes.
    let doc = api_doc_json();
    let security = doc["paths"]["/api/devices"]["get"]["security"]
        .as_array()
        .expect("admin-gated endpoint must declare security")
        .clone();
    let names: Vec<String> = security
        .iter()
        .flat_map(|entry| {
            entry
                .as_object()
                .into_iter()
                .flat_map(|obj| obj.keys().cloned())
        })
        .collect();
    assert!(
        names.iter().any(|n| n == "session_cookie") && names.iter().any(|n| n == "bearer_auth"),
        "admin-gated endpoint should reference both schemes, got {names:?}"
    );
}

/// Visit every operation in the serialized spec, yielding
/// `(path, method, operation)` for each HTTP-method entry under `paths`.
fn for_each_operation(
    doc: &serde_json::Value,
    mut visit: impl FnMut(&str, &str, &serde_json::Value),
) {
    const METHODS: [&str; 8] = [
        "get", "put", "post", "delete", "patch", "head", "options", "trace",
    ];
    let paths = doc["paths"].as_object().expect("paths block must exist");
    for (path, item) in paths {
        for method in METHODS {
            let op = &item[method];
            if !op.is_null() {
                visit(path, method, op);
            }
        }
    }
}

#[test]
fn every_security_requirement_references_a_registered_scheme() {
    // The spot-check above only covers /api/devices. This walks the whole
    // document: any scheme name an operation references must exist in
    // `components.securitySchemes`, otherwise generated clients silently
    // send no credentials and validators reject the spec.
    let doc = api_doc_json();
    let registered: Vec<String> = doc["components"]["securitySchemes"]
        .as_object()
        .expect("securitySchemes block must exist")
        .keys()
        .cloned()
        .collect();

    for_each_operation(&doc, |path, method, op| {
        // Every operation must declare security explicitly — a handler whose
        // `#[utoipa::path]` omits the attribute serializes with no `security`
        // key at all, and generated clients would send no credentials to it.
        // Public endpoints opt out visibly with `security(())`.
        let requirements = op["security"].as_array().unwrap_or_else(|| {
            panic!(
                "{method} {path} declares no security requirements; every \
                 operation needs a security(...) attribute — use security(()) \
                 for deliberately unauthenticated endpoints"
            )
        });
        for entry in requirements {
            let entry = entry.as_object().unwrap_or_else(|| {
                panic!("{method} {path}: security requirement entry must be an object, got {entry}")
            });
            // An empty `{}` object is the no-auth marker and yields no keys.
            for name in entry.keys() {
                assert!(
                    registered.iter().any(|r| r == name),
                    "{method} {path} references security scheme {name:?}, \
                     which is not registered (have: {registered:?})"
                );
            }
        }
    });
}

#[test]
fn operation_tags_match_the_declared_tag_list() {
    // Two-way check: every tag an operation uses must be declared on ApiDoc
    // (otherwise it renders undescribed in Scalar), and every declared tag
    // must be used by at least one operation (otherwise it's dead weight).
    let doc = api_doc_json();
    let declared: std::collections::BTreeSet<String> = doc["tags"]
        .as_array()
        .expect("top-level tags block must exist")
        .iter()
        .map(|t| t["name"].as_str().expect("tag name is a string").to_owned())
        .collect();

    let mut used = std::collections::BTreeSet::new();
    for_each_operation(&doc, |_, _, op| {
        for tag in op["tags"].as_array().into_iter().flatten() {
            used.insert(tag.as_str().expect("operation tag is a string").to_owned());
        }
    });

    assert_eq!(
        used,
        declared,
        "operation tags and ApiDoc's declared tags have drifted apart \
         (missing declarations: {:?}; declared but unused: {:?})",
        used.difference(&declared).collect::<Vec<_>>(),
        declared.difference(&used).collect::<Vec<_>>()
    );
}

#[test]
fn optional_ip_fields_are_nullable_with_accurate_presence() {
    // `Option<Ipv4Addr>` fields carry a manual `value_type` annotation
    // (utoipa has no ToSchema for Ipv4Addr), which makes it easy to copy the
    // non-nullable `String` form from a neighbouring non-Option field and
    // silently publish a client-breaking schema. Pin the contract for the
    // three fields that have already drifted once:
    //
    // - response fields (`gateway`, `foreign_server_ip`) are serialized on
    //   every response (as explicit `null` when unset) → required + nullable;
    // - the request field (`target_ip`) may be omitted by callers entirely
    //   → not required + nullable.
    let doc = api_doc_json();
    let schemas = &doc["components"]["schemas"];

    let is_nullable = |schema: &str, field: &str| {
        let ty = &schemas[schema]["properties"][field]["type"];
        ty.as_array().is_some_and(|t| t.iter().any(|v| v == "null"))
    };
    let is_required = |schema: &str, field: &str| {
        schemas[schema]["required"]
            .as_array()
            .is_some_and(|r| r.iter().any(|v| v == field))
    };

    for (schema, field, required) in [
        ("NetworkStatusResponse", "gateway", true),
        ("DhcpSelfProbeResponse", "foreign_server_ip", true),
        ("DiscoverGatewayMacRequest", "target_ip", false),
    ] {
        assert!(
            is_nullable(schema, field),
            "{schema}.{field} must be nullable in the published schema"
        );
        assert_eq!(
            is_required(schema, field),
            required,
            "{schema}.{field} presence contract drifted: expected required = {required}"
        );
    }
}

// ---------------------------------------------------------------------------
// HTML + static assets
// ---------------------------------------------------------------------------

#[test]
fn scalar_html_wires_runtime_config() {
    // The Scalar docs page must load the spec from the right URL and apply
    // the handful of config knobs we depend on (agent disabled for privacy,
    // developer tools off, brand logo, matching favicon). Asserting as plain
    // substring matches keeps the test readable without needing an HTML/JS
    // parser.
    for needle in [
        "/api/openapi.json",
        "/api/docs/logo.svg",
        "/api/docs/scalar.js",
        "/favicon-32.png",
        "agent: { disabled: true }",
        "mcp: { disabled: true }",
        "showDeveloperTools: 'never'",
        "wardnet-nav",
        "hideDarkModeToggle: true",
        "Scalar.createApiReference",
    ] {
        assert!(
            crate::openapi::SCALAR_HTML.contains(needle),
            "SCALAR_HTML missing required snippet: {needle}"
        );
    }
}

#[test]
fn scalar_bundle_is_embedded_and_looks_like_the_upstream_standalone() {
    let bytes = crate::openapi::SCALAR_JS;
    assert!(
        bytes.len() > 100_000,
        "vendored Scalar bundle looks truncated: {} bytes (expected ~3.6 MB)",
        bytes.len()
    );
    let head = std::str::from_utf8(&bytes[..256.min(bytes.len())]).unwrap_or("");
    // A couple of cheap signals that this is actually the @scalar/api-reference
    // standalone bundle and not a random placeholder.
    assert!(
        head.contains("Scalar") || bytes.windows(7).any(|w| w == b"Scalar "),
        "bundle doesn't appear to reference Scalar in the first 256 bytes or anywhere in-file"
    );
}

#[tokio::test]
async fn scalar_js_rejects_unauthenticated_callers() {
    assert_eq!(
        unauth_status("/api/docs/scalar.js").await,
        StatusCode::UNAUTHORIZED
    );
}

#[test]
fn scalar_logo_svg_is_non_empty_and_looks_like_svg() {
    let bytes = crate::openapi::LOGO_SVG;
    assert!(
        bytes.len() > 512,
        "vendored logo SVG looks truncated: {} bytes",
        bytes.len()
    );
    let text = std::str::from_utf8(bytes).expect("logo SVG should be valid UTF-8");
    assert!(
        text.contains("<svg"),
        "logo doesn't contain an <svg> element"
    );
}

// ---------------------------------------------------------------------------
// Router-level: everything admin-gated
// ---------------------------------------------------------------------------

async fn unauth_status(path: &str) -> StatusCode {
    let app = crate::api::router(test_app_state());
    let req = Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .expect("valid request");
    app.oneshot(req)
        .await
        .expect("router should respond")
        .status()
}

#[tokio::test]
async fn openapi_json_rejects_unauthenticated_callers() {
    assert_eq!(
        unauth_status("/api/openapi.json").await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn scalar_docs_page_rejects_unauthenticated_callers() {
    assert_eq!(unauth_status("/api/docs").await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn scalar_logo_rejects_unauthenticated_callers() {
    assert_eq!(
        unauth_status("/api/docs/logo.svg").await,
        StatusCode::UNAUTHORIZED
    );
}
