use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use tower::{Layer, Service};

tokio::task_local! {
    /// Task-scoped request ID.
    ///
    /// Set by [`RequestContextLayer`] middleware before the request reaches
    /// handlers. Services read it via [`current_request_id`].
    static REQUEST_ID: String;
}

/// Return the request ID for the current request, or `None` if the
/// task-local is not set (e.g. in background tasks or tests).
#[must_use]
pub fn current_request_id() -> Option<String> {
    REQUEST_ID.try_with(std::clone::Clone::clone).ok()
}

/// Run an async block with the given request ID set as the task-local.
///
/// Useful in tests that need to establish a request context.
pub async fn with_request_id<F: Future>(id: String, f: F) -> F::Output {
    REQUEST_ID.scope(id, f).await
}

// -- Tower Layer / Service ---------------------------------------------------

/// Tower layer that wraps each request future in a request-ID scope.
///
/// The request ID is read from the request extensions (inserted by the
/// `inject_request_context` middleware). If no ID is present, an empty
/// string is used as fallback.
#[derive(Clone)]
pub struct RequestContextLayer;

impl<S> Layer<S> for RequestContextLayer {
    type Service = RequestContextMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestContextMiddleware { inner }
    }
}

/// Middleware service that sets the task-local request ID.
#[derive(Clone)]
pub struct RequestContextMiddleware<S> {
    inner: S,
}

/// Newtype stored in request extensions so the middleware can pass
/// the generated request ID through the tower stack.
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

impl<S> Service<Request<Body>> for RequestContextMiddleware<S>
where
    S: Service<Request<Body>, Response = Response> + Send + Clone + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let id = req
            .extensions()
            .get::<RequestId>()
            .map(|r| r.0.clone())
            .unwrap_or_default();

        let mut inner = self.inner.clone();
        Box::pin(REQUEST_ID.scope(id, async move { inner.call(req).await }))
    }
}
