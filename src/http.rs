//! HTTP client for the k9o SDK — reqwest async.
//!
//! Rust idiomatic shape: returns `Result<Response<T>,
//! KError>`. Generic over the data shape via serde::Deserialize on T.

use crate::envelope::{Envelope, Response};
use crate::kerror::{KError, KErrorWire};
use reqwest::{Client as ReqwestClient, Method};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// Backoff strategy for retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backoff {
    Exponential,
    Linear,
    None,
}

/// Retry + backoff policy. Applied to idempotent methods
/// (GET/HEAD/OPTIONS/DELETE) on transient failures only (transport error /
/// 502 / 503 / 504 / 429); semantic 4xx + non-idempotent methods are never
/// retried.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub backoff: Backoff,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff: Backoff::Exponential,
            initial_delay_ms: 100,
            max_delay_ms: 5000,
        }
    }
}

/// Per-instance HTTP config.
#[derive(Debug, Clone)]
pub(crate) struct HttpClientConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub org_slug: Option<String>,
    pub project_slug: Option<String>,
    pub default_headers: HashMap<String, String>,
    pub reqwest: Arc<ReqwestClient>,
    pub retry: RetryConfig,
    /// Regional host resolution state (probe transport + cached origin).
    pub region: Arc<crate::region::RegionState>,
}

fn is_retryable(err: &KError, method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::DELETE
    ) && (err.code == "K9O_UPSTREAM_UNAVAILABLE"
        || matches!(err.status, Some(502 | 503 | 504 | 429)))
}

/// Cheap, non-cryptographic jitter factor in [-1.0, 1.0) from system-time
/// nanos — retry jitter does not need a strong RNG, and this avoids a
/// `rand` dependency.
fn jitter_factor() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1000) as f64 / 500.0 - 1.0
}

/// Backoff delay (ms) for a 0-based attempt index, capped + ±25% jittered.
fn compute_delay_ms(cfg: &RetryConfig, attempt: u32) -> u64 {
    if cfg.backoff == Backoff::None {
        return 0;
    }
    let base = match cfg.backoff {
        Backoff::Linear => cfg.initial_delay_ms.saturating_mul(attempt as u64 + 1),
        _ => cfg
            .initial_delay_ms
            .saturating_mul(1u64.checked_shl(attempt).unwrap_or(u64::MAX)),
    };
    let capped = base.min(cfg.max_delay_ms);
    let jittered = capped as f64 + capped as f64 * 0.25 * jitter_factor();
    jittered.max(0.0) as u64
}

/// Issue a request against the gateway, with retry on transient failures.
///
/// Idempotent methods are retried with backoff per `config.retry`
///; semantic / non-idempotent failures return immediately.
/// Generic on `T` (response data shape) + `B` (body shape).
pub(crate) async fn request<T: DeserializeOwned, B: Serialize + ?Sized>(
    config: &HttpClientConfig,
    method: Method,
    path: &str,
    body: Option<&B>,
    query: Option<&[(String, String)]>,
) -> Result<Response<T>, KError> {
    request_with_headers::<T, B>(config, method, path, body, query, None).await
}

/// Like [`request`], plus caller-supplied per-request headers (e.g. the
/// vault-secrets `X-K9O-Reveal-Challenge` header on `reveal`).
pub(crate) async fn request_with_headers<T: DeserializeOwned, B: Serialize + ?Sized>(
    config: &HttpClientConfig,
    method: Method,
    path: &str,
    body: Option<&B>,
    query: Option<&[(String, String)]>,
    headers: Option<&[(String, String)]>,
) -> Result<Response<T>, KError> {
    request_inner::<T, B>(config, method, path, body, query, headers, None).await
}

/// A pre-encoded request body with its own `content-type` (multipart upload).
/// The content type is set verbatim (it carries the boundary); the JSON
/// serialiser is not involved.
pub(crate) struct RawBody {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// Like [`request`], but sends `raw` as the body instead of JSON.
pub(crate) async fn request_raw<T: DeserializeOwned>(
    config: &HttpClientConfig,
    method: Method,
    path: &str,
    raw: RawBody,
) -> Result<Response<T>, KError> {
    request_inner::<T, ()>(config, method, path, None, None, None, Some(&raw)).await
}

async fn request_inner<T: DeserializeOwned, B: Serialize + ?Sized>(
    config: &HttpClientConfig,
    method: Method,
    path: &str,
    body: Option<&B>,
    query: Option<&[(String, String)]>,
    headers: Option<&[(String, String)]>,
    raw: Option<&RawBody>,
) -> Result<Response<T>, KError> {
    let cfg = &config.retry;
    let mut attempt: u32 = 0;
    loop {
        match request_once::<T, B>(config, method.clone(), path, body, query, headers, raw).await {
            Ok(r) => return Ok(r),
            Err(e) => {
                if attempt >= cfg.max_retries || !is_retryable(&e, &method) {
                    return Err(e);
                }
                let delay = compute_delay_ms(cfg, attempt);
                if delay > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
                attempt += 1;
            }
        }
    }
}

/// Single-attempt request. `request`/`request_with_headers` wrap this in
/// retry-with-backoff.
async fn request_once<T: DeserializeOwned, B: Serialize + ?Sized>(
    config: &HttpClientConfig,
    method: Method,
    path: &str,
    body: Option<&B>,
    query: Option<&[(String, String)]>,
    headers: Option<&[(String, String)]>,
    raw: Option<&RawBody>,
) -> Result<Response<T>, KError> {
    let origin = crate::region::resolve_origin(&config.base_url, &config.region).await;
    let url = format!("{}{}", origin, path);

    let mut req = config.reqwest.request(method, &url);
    req = req.header("accept", "application/json");
    for (k, v) in &config.default_headers {
        req = req.header(k, v);
    }
    if let Some(slug) = &config.project_slug {
        req = req.header("x-k9o-project-slug", slug);
    }
    if let Some(slug) = &config.org_slug {
        req = req.header("x-k9o-org-slug", slug);
    }
    if let Some(token) = &config.api_key {
        req = req.header("authorization", format!("Bearer {}", token));
    }
    if let Some(hs) = headers {
        for (k, v) in hs {
            req = req.header(k.as_str(), v.as_str());
        }
    }
    if let Some(r) = raw {
        req = req
            .header("content-type", r.content_type.as_str())
            .body(r.bytes.clone());
    } else if let Some(b) = body {
        req = req.json(b);
    }
    if let Some(q) = query {
        req = req.query(q);
    }

    let response = req.send().await.map_err(|e| {
        KError::new(
            "K9O_UPSTREAM_UNAVAILABLE",
            format!("Network error reaching {}: {}", url, e),
        )
        .with_context("url", url.clone())
        .with_recommendation("Check connectivity to the gateway; retry with backoff if transient.")
    })?;

    let status = response.status();

    // 204 No Content
    if status == 204 {
        // Synthesize a minimal envelope. Caller's T must be deserializable
        // from null + empty meta.
        let bytes = b"{\"data\":null,\"meta\":{}}";
        let env: Envelope<T> = serde_json::from_slice(bytes).map_err(|e| {
            KError::new(
                "K9O_INTERNAL_ERROR",
                format!("204 No Content but T cannot deserialize null: {}", e),
            )
        })?;
        return Ok(env);
    }

    let bytes = response.bytes().await.map_err(|e| {
        KError::new(
            "K9O_INTERNAL_ERROR",
            format!("Failed to read response body from {}: {}", url, e),
        )
    })?;

    if status.is_success() {
        let env: Envelope<T> = serde_json::from_slice(&bytes).map_err(|e| {
            KError::new(
                "K9O_INTERNAL_ERROR",
                format!("Malformed envelope from {}: {}", url, e),
            )
            .with_context("url", url.clone())
            .with_context("status", status.as_u16())
        })?;
        return Ok(env);
    }

    // Non-2xx — try KError wire shape
    if let Ok(wire) = serde_json::from_slice::<KErrorWire>(&bytes) {
        return Err(wire.into_kerror(status.as_u16()));
    }
    Err(KError::new(
        "K9O_INTERNAL_ERROR",
        format!("Upstream returned {} with non-KError body", status.as_u16()),
    )
    .with_context("url", url)
    .with_context("status", status.as_u16()))
}
