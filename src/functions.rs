//! Function lifecycle namespace — `client.functions()`.
//!
//! Wire mapping:
//!   - `upload_artifact(bytes, kind)` -> `POST /@<slug>/functions/artifacts` (raw bytes)
//!   - `deploy(input)`                -> `POST /@<slug>/functions`
//!   - `list(opts)`                   -> `GET /@<slug>/functions`
//!   - `get(name)` / `delete(name)`   -> `GET` / `DELETE /@<slug>/functions/<name>`
//!   - `invoke(name, payload)`        -> `POST /@<slug>/functions/<name>/invoke`

use crate::envelope::Response;
use crate::http::{request, request_raw, HttpClientConfig, RawBody};
use crate::kerror::KError;
use crate::stream::require_project;
use crate::url_grammar::{enc, project_path};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Function record returned by gateway endpoints.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionRecord {
    pub id: String,
    pub name: String,
    pub runtime: String,
    pub tier: String,
    #[serde(default)]
    pub entry_point: Option<String>,
    #[serde(default)]
    pub version: String,
    pub status: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub description: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub deployed_at: Option<String>,
    #[serde(default)]
    pub retired_at: Option<String>,
    #[serde(default)]
    pub error_detail: Option<String>,
}

/// Deploy input. The function's bytes are uploaded first
/// ([`FunctionsNamespace::upload_artifact`] returns the `artifact_key`);
/// deploy references that artifact.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployFunctionInput<'a> {
    /// Function name (1 to 128 characters).
    pub name: &'a str,
    /// Runtime, e.g. `"bun"`, `"python"`, `"go"`, `"rust"`.
    pub runtime: &'a str,
    /// `"gateway"` (WebAssembly in the gateway) or `"sandbox"` (an OCI image archive).
    pub tier: &'a str,
    /// Content-addressed key of the uploaded artifact.
    pub artifact_key: &'a str,
    /// Version label for this deploy (1 to 64 characters).
    pub version: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
}

/// Optional query filters of `list`.
#[derive(Debug, Default, Clone)]
pub struct ListFunctionsOptions<'a> {
    pub limit: Option<u32>,
    pub cursor: Option<&'a str>,
    /// `"deploying"`, `"active"`, `"error"` or `"retired"`.
    pub status: Option<&'a str>,
    pub source: Option<&'a str>,
    pub tier: Option<&'a str>,
}

/// Optional invoke controls; property names are the gateway's.
#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvokeOptions<'a> {
    /// Run asynchronously instead of waiting for the result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegation: Option<InvokeDelegation<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_behalf_of: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confined_to: Option<&'a str>,
}

/// A delegation token for an invoke.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvokeDelegation<'a> {
    pub token: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoke_session_id: Option<&'a str>,
}

/// Result of `upload_artifact`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactUpload {
    pub artifact_key: String,
}

#[derive(Serialize)]
struct InvokeBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<&'a Value>,
    #[serde(flatten)]
    options: &'a InvokeOptions<'a>,
}

/// Functions namespace — `client.functions()`.
pub struct FunctionsNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl<'a> FunctionsNamespace<'a> {
    /// Upload a function artifact and get back its content-addressed
    /// `artifact_key`, to pass to `deploy`: `POST /@<slug>/functions/artifacts`
    /// with the raw bytes. `kind` is `"wasm"` (a WebAssembly module for the
    /// gateway tier, up to 16 MiB) or `"tar"` (an OCI image archive for the
    /// sandbox tier, up to 256 MiB). Uploading the same bytes again returns
    /// the same key.
    pub async fn upload_artifact(
        &self,
        bytes: Vec<u8>,
        kind: &str,
    ) -> Result<Response<ArtifactUpload>, KError> {
        let slug = require_project(self.config, "functions")?;
        let content_type = match kind {
            "wasm" => "application/wasm",
            "tar" => "application/x-tar",
            _ => {
                return Err(KError::new(
                    "K9O_INVALID_FIELD_VALUE",
                    "artifact kind must be \"wasm\" or \"tar\"",
                )
                .with_context("field", "kind"))
            }
        };
        request_raw::<ArtifactUpload>(
            self.config,
            Method::POST,
            &project_path(slug, "functions/artifacts"),
            RawBody {
                content_type: content_type.to_string(),
                bytes,
            },
        )
        .await
    }

    /// Deploy or redeploy a function from an uploaded artifact. The record
    /// comes back with status `deploying`; it is ready when it flips to `active`.
    pub async fn deploy(
        &self,
        input: DeployFunctionInput<'_>,
    ) -> Result<Response<FunctionRecord>, KError> {
        let slug = require_project(self.config, "functions")?;
        request::<FunctionRecord, _>(
            self.config,
            Method::POST,
            &project_path(slug, "functions"),
            Some(&input),
            None,
        )
        .await
    }

    /// List deployed functions (cursor pagination).
    pub async fn list(
        &self,
        opts: ListFunctionsOptions<'_>,
    ) -> Result<Response<Vec<FunctionRecord>>, KError> {
        let slug = require_project(self.config, "functions")?;
        let mut q: Vec<(String, String)> = Vec::new();
        if let Some(l) = opts.limit {
            q.push(("limit".into(), l.to_string()));
        }
        if let Some(c) = opts.cursor {
            q.push(("cursor".into(), c.into()));
        }
        if let Some(s) = opts.status {
            q.push(("status".into(), s.into()));
        }
        if let Some(s) = opts.source {
            q.push(("source".into(), s.into()));
        }
        if let Some(t) = opts.tier {
            q.push(("tier".into(), t.into()));
        }
        request::<Vec<FunctionRecord>, ()>(
            self.config,
            Method::GET,
            &project_path(slug, "functions"),
            None,
            Some(&q),
        )
        .await
    }

    pub async fn get(&self, name: &str) -> Result<Response<FunctionRecord>, KError> {
        let slug = require_project(self.config, "functions")?;
        request::<FunctionRecord, ()>(
            self.config,
            Method::GET,
            &project_path(slug, &format!("functions/{}", enc(name))),
            None,
            None,
        )
        .await
    }

    /// Retire a function. The result carries `retired` and `cooldownUntil`; the
    /// name enters a 24h cooldown.
    pub async fn delete(&self, name: &str) -> Result<Response<Value>, KError> {
        let slug = require_project(self.config, "functions")?;
        request::<Value, ()>(
            self.config,
            Method::DELETE,
            &project_path(slug, &format!("functions/{}", enc(name))),
            None,
            None,
        )
        .await
    }

    /// Invoke a function. `payload` is sent as the `payload` field of the
    /// request body and passed to the function runtime verbatim (omitted when
    /// `None`); `options` add the gateway's other invoke controls.
    pub async fn invoke(
        &self,
        name: &str,
        payload: Option<&Value>,
        options: Option<&InvokeOptions<'_>>,
    ) -> Result<Response<Value>, KError> {
        let slug = require_project(self.config, "functions")?;
        let default = InvokeOptions::default();
        let body = InvokeBody {
            payload,
            options: options.unwrap_or(&default),
        };
        request::<Value, _>(
            self.config,
            Method::POST,
            &project_path(slug, &format!("functions/{}/invoke", enc(name))),
            Some(&body),
            None,
        )
        .await
    }
}
