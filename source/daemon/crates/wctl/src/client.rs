//! Thin `reqwest` wrapper that speaks the daemon's JSON API.
//!
//! Every request carries `Authorization: Bearer <token>`; every non-2xx
//! response is decoded into a [`wardnet_common::api::ApiError`] and surfaced
//! as [`CliError::Api`], so callers only ever see typed results.

use std::time::Duration;

use reqwest::{RequestBuilder, Response, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use wardnet_common::api::ApiError;

use crate::error::CliError;

/// An authenticated HTTP client bound to one daemon.
pub struct Client {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl Client {
    /// Build a client for `base_url` (no trailing slash) using `token` as the
    /// bearer credential.
    ///
    /// # Errors
    /// Returns [`CliError::Connection`] if the underlying HTTP client cannot
    /// be constructed.
    pub fn new(base_url: &str, token: &str) -> Result<Self, CliError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_mins(2))
            .build()
            .map_err(|e| CliError::Connection(format!("failed to build HTTP client: {e}")))?;
        Ok(Self {
            http,
            base: base_url.to_owned(),
            token: token.to_owned(),
        })
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn authed(&self, builder: RequestBuilder) -> RequestBuilder {
        builder.bearer_auth(&self.token)
    }

    async fn send(&self, builder: RequestBuilder) -> Result<Response, CliError> {
        self.authed(builder)
            .send()
            .await
            .map_err(|e| CliError::Connection(format!("cannot reach daemon at {}: {e}", self.base)))
    }

    /// `GET {path}` decoded as `T`.
    ///
    /// # Errors
    /// [`CliError`] on transport failure or a non-2xx response.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, CliError> {
        let resp = self.send(self.http.get(self.url(path))).await?;
        decode_json(resp).await
    }

    /// `POST {path}` with no body, decoded as `T`.
    ///
    /// # Errors
    /// [`CliError`] on transport failure or a non-2xx response.
    pub async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, CliError> {
        let resp = self.send(self.http.post(self.url(path))).await?;
        decode_json(resp).await
    }

    /// `POST {path}` with a JSON body, decoded as `T`.
    ///
    /// # Errors
    /// [`CliError`] on transport failure or a non-2xx response.
    pub async fn post_json<B, T>(&self, path: &str, body: &B) -> Result<T, CliError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let resp = self.send(self.http.post(self.url(path)).json(body)).await?;
        decode_json(resp).await
    }

    /// `PUT {path}` with a JSON body, decoded as `T`.
    ///
    /// # Errors
    /// [`CliError`] on transport failure or a non-2xx response.
    pub async fn put_json<B, T>(&self, path: &str, body: &B) -> Result<T, CliError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let resp = self.send(self.http.put(self.url(path)).json(body)).await?;
        decode_json(resp).await
    }

    /// `DELETE {path}`, decoded as `T`.
    ///
    /// # Errors
    /// [`CliError`] on transport failure or a non-2xx response.
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, CliError> {
        let resp = self.send(self.http.delete(self.url(path))).await?;
        decode_json(resp).await
    }

    /// `POST {path}` with a JSON body, returning the raw response bytes.
    ///
    /// Used for the backup export endpoint, which streams an
    /// `application/octet-stream` bundle rather than JSON.
    ///
    /// # Errors
    /// [`CliError`] on transport failure or a non-2xx response.
    pub async fn post_json_bytes<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<Vec<u8>, CliError> {
        let resp = self.send(self.http.post(self.url(path)).json(body)).await?;
        let status = resp.status();
        let bytes = read_body(resp).await?;
        if status.is_success() {
            Ok(bytes)
        } else {
            Err(api_error(status, &bytes))
        }
    }

    /// `POST {path}` with a `multipart/form-data` body, decoded as `T`.
    ///
    /// # Errors
    /// [`CliError`] on transport failure or a non-2xx response.
    pub async fn post_multipart<T: DeserializeOwned>(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<T, CliError> {
        let resp = self
            .send(self.http.post(self.url(path)).multipart(form))
            .await?;
        decode_json(resp).await
    }
}

/// Read a response body to bytes, mapping transport errors to
/// [`CliError::Connection`].
async fn read_body(resp: Response) -> Result<Vec<u8>, CliError> {
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| CliError::Connection(format!("reading response body failed: {e}")))
}

/// Decode a JSON response, or turn a non-2xx into a [`CliError::Api`].
async fn decode_json<T: DeserializeOwned>(resp: Response) -> Result<T, CliError> {
    let status = resp.status();
    let bytes = read_body(resp).await?;
    if status.is_success() {
        serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Protocol(format!("unexpected response from daemon: {e}")))
    } else {
        Err(api_error(status, &bytes))
    }
}

/// Build a [`CliError::Api`] from an error response, falling back to the HTTP
/// status text when the body is not a JSON [`ApiError`].
pub(crate) fn api_error(status: StatusCode, bytes: &[u8]) -> CliError {
    let body = serde_json::from_slice::<ApiError>(bytes).unwrap_or_else(|_| {
        let detail = std::str::from_utf8(bytes)
            .ok()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        ApiError {
            error: status
                .canonical_reason()
                .unwrap_or("request failed")
                .to_lowercase(),
            detail,
            request_id: None,
        }
    });
    CliError::Api {
        status: status.as_u16(),
        body,
    }
}
