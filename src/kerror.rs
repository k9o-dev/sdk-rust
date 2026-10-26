//! KError — the closed-taxonomy error type returned by every k9o
//! surface on non-2xx gateway responses or SDK-side validation failures.
//!
//! Rust idiomatic shape `Result<Response<T>, KError>`.
//! KError implements `std::error::Error` via thiserror; safe to use
//! with `?` operator + `anyhow`/`eyre`/etc.

use serde::Deserialize;
use std::collections::HashMap;
use thiserror::Error;

/// The customer-facing error type returned by k9o surfaces.
///
/// Code-narrowing: caller pattern-matches on `code.as_str()` per the
/// closed taxonomy. Open code-space — unknown codes pass
/// through verbatim (gateway-curated).
#[derive(Debug, Clone, Error)]
#[error("[{code}] {message}{status_suffix}", status_suffix = match status { Some(s) => format!(" (status={})", s), None => String::new() })]
pub struct KError {
    /// Closed-taxonomy code (e.g. "K9O_TIER_CAP_HIT").
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
    /// Structured per-code payload.
    pub context: HashMap<String, serde_json::Value>,
    /// Actionable next step.
    pub recommendation: Option<String>,
    /// HTTP status (None for SDK-side errors).
    pub status: Option<u16>,
    /// Gateway-emitted trace id.
    pub request_id: Option<String>,
}

impl KError {
    /// Construct a SDK-side KError (no HTTP status).
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            context: HashMap::new(),
            recommendation: None,
            status: None,
            request_id: None,
        }
    }

    /// Attach a recommendation.
    pub fn with_recommendation(mut self, rec: impl Into<String>) -> Self {
        self.recommendation = Some(rec.into());
        self
    }

    /// Attach a context key/value.
    pub fn with_context(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}

/// Wire-shape sentinel matching the gateway's KError JSON payload per
/// JSON tags handle wire camelCase → Rust snake_case
/// translation at the boundary
#[derive(Debug, Deserialize)]
pub(crate) struct KErrorWire {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub context: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub recommendation: Option<String>,
    #[serde(rename = "requestId", default)]
    pub request_id: Option<String>,
}

impl KErrorWire {
    /// Rehydrate the wire payload into a typed KError with HTTP status.
    pub(crate) fn into_kerror(self, status: u16) -> KError {
        KError {
            code: self.code,
            message: self.message,
            context: self.context,
            recommendation: self.recommendation,
            status: Some(status),
            request_id: self.request_id,
        }
    }
}
