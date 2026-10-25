//! Realtime Subscriptions namespace — `client.realtime()`
//! (ALL-LIVE primitive).
//!
//! `subscribe` returns a [`Subscription`] async-iterated via `next().await`
//!. Per + `feedback_sdk_abstracted_primitive_default_to
//! _richer_interaction.md`, the SDK picks the transport; SPLIT (polling) is
//! the internal fallback. The reqwest-based Rust SDK uses SSE primary +
//! poll fallback (no stdlib WebSocket); the consumer's `next()` loop is
//! transport-agnostic. `replay` is the one-shot historical catch-up.

use crate::envelope::Response;
use crate::http::{request, HttpClientConfig};
use crate::kerror::KError;
use crate::stream::{open_sse_stream, require_project, SseStream};
use crate::url_grammar::project_path;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::time::Duration;

/// A single realtime event delivered on a subscription.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeEvent {
    pub topic: String,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub sequence: i64,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub occurred_at: String,
}

/// Input for `replay`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayInput<'a> {
    pub topic: &'a str,
    pub from_sequence: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until_sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Transport selection for `subscribe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// SSE (default) — richest portable transport in the reqwest SDK.
    Sse,
    /// Poll fallback — repeated `replay` calls (SDK-internal SPLIT).
    Poll,
}

impl Default for Transport {
    fn default() -> Self {
        Transport::Sse
    }
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

enum SubInner<'a> {
    Sse(SseStream<RealtimeEvent>),
    Poll {
        config: &'a HttpClientConfig,
        slug: String,
        topic: String,
        filter: Option<Value>,
        cursor: i64,
        interval: Duration,
        buffer: VecDeque<RealtimeEvent>,
    },
}

/// A live subscription. Iterate with `next().await`; the SDK transport
/// (SSE primary / poll fallback) is selected internally.
pub struct Subscription<'a> {
    inner: SubInner<'a>,
}

impl<'a> Subscription<'a> {
    /// Yield the next event, or `None` when an SSE stream ends.
    /// `Some(Err(_))` on transport error. Poll-backed subscriptions block
    /// until the next event arrives (drop the subscription to stop).
    pub async fn next(&mut self) -> Option<Result<RealtimeEvent, KError>> {
        match &mut self.inner {
            SubInner::Sse(stream) => stream.next().await,
            SubInner::Poll {
                config,
                slug,
                topic,
                filter,
                cursor,
                interval,
                buffer,
            } => loop {
                if let Some(ev) = buffer.pop_front() {
                    return Some(Ok(ev));
                }
                let body = serde_json::json!({
                    "topic": topic,
                    "filter": filter,
                    "fromSequence": *cursor,
                });
                let path = project_path(slug, "realtime/replay");
                match request::<Vec<RealtimeEvent>, Value>(
                    config,
                    Method::POST,
                    &path,
                    Some(&body),
                    None,
                )
                .await
                {
                    Ok(resp) => {
                        for ev in resp.data {
                            if ev.sequence + 1 > *cursor {
                                *cursor = ev.sequence + 1;
                            }
                            buffer.push_back(ev);
                        }
                        if buffer.is_empty() {
                            tokio::time::sleep(*interval).await;
                        }
                    }
                    Err(e) => return Some(Err(e)),
                }
            },
        }
    }
}

/// Realtime namespace — `client.realtime()`.
pub struct RealtimeNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl<'a> RealtimeNamespace<'a> {
    /// Subscribe to a topic. Iterate the returned [`Subscription`] with
    /// `next().await`. `poll_interval` applies only to the poll transport
    /// (defaults to 2s).
    pub async fn subscribe(
        &self,
        topic: &str,
        filter: Option<Value>,
        transport: Transport,
        poll_interval: Option<Duration>,
    ) -> Result<Subscription<'a>, KError> {
        let slug = require_project(self.config, "realtime")?;
        match transport {
            Transport::Poll => Ok(Subscription {
                inner: SubInner::Poll {
                    config: self.config,
                    slug: slug.to_string(),
                    topic: topic.to_string(),
                    filter,
                    cursor: 0,
                    interval: poll_interval.unwrap_or_else(|| Duration::from_secs(2)),
                    buffer: VecDeque::new(),
                },
            }),
            Transport::Sse => {
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
                Ok(Subscription {
                    inner: SubInner::Sse(stream),
                })
            }
        }
    }

    /// One-shot historical replay (capped 10k server-side).
    pub async fn replay(
        &self,
        input: ReplayInput<'_>,
    ) -> Result<Response<Vec<RealtimeEvent>>, KError> {
        let slug = require_project(self.config, "realtime")?;
        if input.from_sequence < 0 {
            return Err(KError::new(
                "K9O_INVALID_FILTER",
                "replay from_sequence must be a non-negative integer",
            )
            .with_recommendation("Pass the highest sequence number you have already observed (or 0 to replay from the start)."));
        }
        request::<Vec<RealtimeEvent>, _>(
            self.config,
            Method::POST,
            &project_path(slug, "realtime/replay"),
            Some(&input),
            None,
        )
        .await
    }
}
