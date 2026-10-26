//! Realtime Subscriptions namespace — `client.realtime()`.
//!
//! `subscribe` returns a [`Subscription`] async-iterated via `next().await`.
//! Transport: SSE over `GET /@<slug>/realtime/subscribe`. The gateway serves
//! no WebSocket upgrade and no durable replay, so neither is offered; after an
//! `event: resync` frame the caller re-reads current state from the data plane
//! and resubscribes.

use crate::http::HttpClientConfig;
use crate::kerror::KError;
use crate::stream::{open_sse_stream, require_project, SseStream};
use crate::url_grammar::project_path;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single realtime event delivered on a subscription.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeEvent {
    pub topic: String,
    pub event_type: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub occurred_at: String,
}

/// Typed builders for the 6 closed topic families.
pub mod topic {
    pub fn collection(collection: &str) -> String {
        format!("collections.{}.*", collection)
    }
    pub fn workflow_runs() -> String {
        "studio.workflows.runs.*".to_string()
    }
    pub fn query_executed(query_id: &str) -> String {
        format!("studio.queries.executed.{}", query_id)
    }
    pub fn dashboard_refresh(dashboard_id: &str) -> String {
        format!("studio.dashboards.refresh.{}", dashboard_id)
    }
    pub fn function_invocations(function_id: &str) -> String {
        format!("forge.functions.invocations.{}", function_id)
    }
    pub fn audit_org(event_family: &str) -> String {
        format!("audit.org.{}", event_family)
    }
}

/// A live subscription over SSE. Iterate with `next().await`.
pub struct Subscription {
    stream: SseStream<RealtimeEvent>,
}

impl Subscription {
    /// Yield the next event, or `None` when the stream ends. `Some(Err(_))` on
    /// a transport error. After an `event: resync` frame, re-read current state
    /// from the data plane and subscribe again.
    pub async fn next(&mut self) -> Option<Result<RealtimeEvent, KError>> {
        self.stream.next().await
    }
}

/// Realtime namespace — `client.realtime()`.
pub struct RealtimeNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl<'a> RealtimeNamespace<'a> {
    /// Subscribe to a topic over SSE: `GET /@<slug>/realtime/subscribe?topic=&filter=`.
    /// `filter` narrows the subscription (it composes with RLS: the filter
    /// narrows, RLS gates) and is sent as JSON. The gateway serves no
    /// WebSocket upgrade and no durable replay, so neither is offered.
    pub async fn subscribe(
        &self,
        topic: &str,
        filter: Option<Value>,
    ) -> Result<Subscription, KError> {
        let slug = require_project(self.config, "realtime")?;
        let mut q: Vec<(String, String)> = vec![("topic".into(), topic.into())];
        if let Some(f) = &filter {
            q.push(("filter".into(), f.to_string()));
        }
        let stream = open_sse_stream::<RealtimeEvent, ()>(
            self.config,
            Method::GET,
            &project_path(slug, "realtime/subscribe"),
            None,
            Some(&q),
        )
        .await?;
        Ok(Subscription { stream })
    }
}
