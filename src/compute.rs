//! Compute namespace — `client.compute()` (Compute LLMs
//! registry).
//!
//! Rust idiomatic: `Result<Response<T>, KError>`;
//! `invoke_stream` returns an `SseStream<LlmStreamChunk>` async-iterated
//! via `next().await`.

use crate::envelope::Response;
use crate::http::{request, HttpClientConfig};
use crate::kerror::KError;
use crate::stream::{open_sse_stream, require_project, SseStream};
use crate::url_grammar::project_path;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Model metadata +.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmModel {
    pub id: String,
    pub provider_class: String,
    pub modality: String,
    pub tier_gate: String,
    #[serde(default)]
    pub context_window: u64,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub cost_per_million_input_tokens: f64,
    #[serde(default)]
    pub cost_per_million_output_tokens: f64,
}

/// Synchronous inference result.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmResponse {
    pub completion: String,
    #[serde(default)]
    pub tokens_used: u64,
    #[serde(default)]
    pub latency_ms: u64,
}

/// Streaming per-token chunk.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmStreamChunk {
    #[serde(default)]
    pub delta: String,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub tokens_used: u64,
}

/// Input body for invoke / invoke_stream.
#[derive(Debug, Serialize)]
pub struct LlmInvokeInput<'a> {
    pub prompt: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// Curated closed model enum (extensible v2.x).
pub const LLM_MODEL_IDS: [&str; 9] = [
    "mistral-7b",
    "mistral-8x7b",
    "llama-3-8b",
    "llama-3-70b",
    "claude-3-5-sonnet",
    "gpt-4o",
    "gpt-4o-mini",
    "text-embedding-3-small",
    "text-embedding-3-large",
];

/// LLMs namespace — `client.compute().llms()`.
pub struct LlmsNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl<'a> LlmsNamespace<'a> {
    pub async fn list(&self, modality: Option<&str>) -> Result<Response<Vec<LlmModel>>, KError> {
        let slug = require_project(self.config, "compute.llms")?;
        let q: Vec<(String, String)> = modality
            .map(|m| vec![("modality".into(), m.into())])
            .unwrap_or_default();
        request::<Vec<LlmModel>, ()>(
            self.config,
            Method::GET,
            &project_path(slug, "compute/llms"),
            None,
            Some(&q),
        )
        .await
    }

    pub async fn read(&self, model_id: &str) -> Result<Response<LlmModel>, KError> {
        let slug = require_project(self.config, "compute.llms")?;
        let path = project_path(slug, &format!("compute/llms/{}", model_id));
        request::<LlmModel, ()>(self.config, Method::GET, &path, None, None).await
    }

    /// Synchronous inference (PROJECT_AUTHOR/ADMIN).
    pub async fn invoke(
        &self,
        model_id: &str,
        input: LlmInvokeInput<'_>,
    ) -> Result<Response<LlmResponse>, KError> {
        let slug = require_project(self.config, "compute.llms")?;
        let path = project_path(slug, &format!("compute/llms/{}/invoke", model_id));
        request::<LlmResponse, _>(self.config, Method::POST, &path, Some(&input), None).await
    }

    /// Streaming inference (SSE per-token). Iterate the
    /// returned stream with `next().await`.
    pub async fn invoke_stream(
        &self,
        model_id: &str,
        input: LlmInvokeInput<'_>,
    ) -> Result<SseStream<LlmStreamChunk>, KError> {
        let slug = require_project(self.config, "compute.llms")?;
        let path = project_path(slug, &format!("compute/llms/{}/invoke-stream", model_id));
        open_sse_stream::<LlmStreamChunk, _>(self.config, Method::POST, &path, Some(&input), None)
            .await
    }
}

/// Compute namespace grouping the compute surfaces.
pub struct ComputeNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl<'a> ComputeNamespace<'a> {
    pub fn llms(&self) -> LlmsNamespace<'a> {
        LlmsNamespace {
            config: self.config,
        }
    }
}
