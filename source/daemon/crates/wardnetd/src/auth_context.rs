use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use tower::{Layer, Service};
use wardnet_types::auth::AuthContext;

tokio::task_local! {
    /// Task-scoped authentication context.
    ///
    /// Set by [`AuthContextLayer`] middleware before the request reaches
    /// handlers. Services read it via [`current`].
    static AUTH_CONTEXT: AuthContext;
}

/// Return the [`AuthContext`] for the current request.
///
/// Panics if called outside an [`AuthContextLayer`] scope (should never
/// happen for code reachable from an HTTP handler).
#[must_use]
pub fn current() -> AuthContext {
    AUTH_CONTEXT.with(std::clone::Clone::clone)
}

/// Try to read the current [`AuthContext`], returning `None` if the
/// task-local is not set (e.g. in background tasks or tests).
#[must_use]
pub fn try_current() -> Option<AuthContext> {
    AUTH_CONTEXT.try_with(std::clone::Clone::clone).ok()
}

/// Run an async block with the given [`AuthContext`] set as the task-local.
///
/// Useful in tests and background tasks that need to establish a context.
pub async fn with_context<F: Future>(ctx: AuthContext, f: F) -> F::Output {
    AUTH_CONTEXT.scope(ctx, f).await
}

/// Require that the current caller is an admin.
///
/// Returns `Ok(())` if the task-local [`AuthContext`] is [`AuthContext::Admin`],
/// otherwise returns `Err(AppError::Forbidden)`.
pub fn require_admin() -> Result<(), crate::error::AppError> {
    let ctx = try_current().unwrap_or(AuthContext::Anonymous);
    if !ctx.is_admin() {
        return Err(crate::error::AppError::Forbidden(
            "admin privileges required".to_owned(),
        ));
    }
    Ok(())
}

/// Require that the current caller is authenticated (admin or device).
///
/// Returns `Ok(())` if authenticated, `Err(AppError::Forbidden)` if anonymous
/// or no context is set.
pub fn require_authenticated() -> Result<(), crate::error::AppError> {
    let ctx = try_current().unwrap_or(AuthContext::Anonymous);
    if matches!(ctx, AuthContext::Anonymous) {
        return Err(crate::error::AppError::Forbidden(
            "authentication required".to_owned(),
        ));
    }
    Ok(())
}

// -- Tower Layer / Service --------------------------------------------------

/// Tower layer that wraps each request future in an [`AuthContext`] scope.
///
/// The context is read from the request extensions (inserted by Axum
/// extractors in the middleware). If no context is present, falls back
/// to [`AuthContext::Anonymous`].
#[derive(Clone)]
pub struct AuthContextLayer;

impl<S> Layer<S> for AuthContextLayer {
    type Service = AuthContextMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthContextMiddleware { inner }
    }
}

/// Middleware service that sets the task-local [`AuthContext`].
#[derive(Clone)]
pub struct AuthContextMiddleware<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for AuthContextMiddleware<S>
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
        let ctx = req
            .extensions()
            .get::<AuthContext>()
            .cloned()
            .unwrap_or(AuthContext::Anonymous);

        let mut inner = self.inner.clone();
        Box::pin(AUTH_CONTEXT.scope(ctx, async move { inner.call(req).await }))
    }
}
