//! Regression guard for panic isolation in the HTTP stack.
//!
//! A handler panic must be caught and converted to a `500` so the request is
//! isolated — the listener stays up and other requests are unaffected — rather
//! than unwinding the connection future (which, in the worst case, can take the
//! whole API dark and poison shared state). This mirrors a production incident
//! in wardnet-cloud where a single panicking handler stopped the listener and
//! produced zero log lines.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt;

/// A route handler that always panics. Declares a concrete return type so the
/// diverging `panic!` (`!`) coerces and the closure satisfies axum's `Handler`.
async fn panicking_handler() -> StatusCode {
    panic!("simulated handler panic")
}

/// A handler that panics must surface as a `500 Internal Server Error`, not as
/// an unwinding panic that propagates past the service. Exercises the exact
/// [`catch_panic_layer`](crate::api::catch_panic_layer) the production router
/// applies.
#[tokio::test]
async fn handler_panic_is_isolated_as_500() {
    let app = Router::new()
        .route("/__panic", get(panicking_handler))
        .layer(crate::api::catch_panic_layer());

    let req = Request::builder()
        .method("GET")
        .uri("/__panic")
        .body(Body::empty())
        .expect("valid request");

    // Without panic isolation this `.await` unwinds (the handler panic
    // propagates), failing the test. With the catch-panic layer the service
    // returns a normal `500` response and the await resolves to `Ok`.
    let resp = app
        .oneshot(req)
        .await
        .expect("handler panic must not propagate past the service");

    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
