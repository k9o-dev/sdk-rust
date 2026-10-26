//! Project-plane audit log namespace — `client.audit()`.

use crate::envelope::Response;
use crate::http::{request, HttpClientConfig};
use crate::kerror::KError;
use crate::stream::require_project;
use crate::url_grammar::project_path;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Audit event record
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub actor_id: Option<String>,
    #[serde(default)]
    pub target_kind: Option<String>,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
    pub occurred_at: String,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub trace_id: Option<String>,
}

/// Optional filters for audit queries; names follow the gateway's query parameters.
#[derive(Debug, Default, Clone)]
pub struct AuditQueryOpts<'a> {
    /// Filter by audit-event name (e.g. `data.write.access.insert`).
    pub event_type: Option<&'a str>,
    /// Filter by actor.
    pub actor_id: Option<&'a str>,
    /// ISO 8601 lower bound: only events after this instant.
    pub after: Option<&'a str>,
    /// ISO 8601 upper bound: only events before this instant.
    pub before: Option<&'a str>,
    /// Page size (the per-tier maximum is bounded server-side).
    pub limit: Option<u32>,
    /// Cursor from a prior response.
    pub cursor: Option<&'a str>,
}

impl<'a> AuditQueryOpts<'a> {
    pub(crate) fn to_query(&self) -> Vec<(String, String)> {
        let mut q: Vec<(String, String)> = Vec::new();
        if let Some(v) = self.event_type {
            q.push(("eventType".into(), v.into()));
        }
        if let Some(v) = self.actor_id {
            q.push(("actorId".into(), v.into()));
        }
        if let Some(v) = self.after {
            q.push(("after".into(), v.into()));
        }
        if let Some(v) = self.before {
            q.push(("before".into(), v.into()));
        }
        if let Some(v) = self.limit {
            q.push(("limit".into(), v.to_string()));
        }
        if let Some(v) = self.cursor {
            q.push(("cursor".into(), v.into()));
        }
        q
    }
}

/// Audit namespace — `client.audit()` (project-plane).
pub struct AuditNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl<'a> AuditNamespace<'a> {
    /// Query the audit log (reverse-chronological; filters AND-compose).
    pub async fn query(
        &self,
        opts: AuditQueryOpts<'_>,
    ) -> Result<Response<Vec<AuditEvent>>, KError> {
        let slug = require_project(self.config, "audit")?;
        request::<Vec<AuditEvent>, ()>(
            self.config,
            Method::GET,
            &project_path(slug, "audit-log"),
            None,
            Some(&opts.to_query()),
        )
        .await
    }
}
