//! Approvals namespaces: `client.approvals()` (the pending-change queue) and
//! `client.approval_policies()` (the rules that decide which writes are queued).
//!
//! Wire mapping (project plane):
//!   - `approvals().list(..)`       -> `GET    /@<slug>/pending-approvals`
//!   - `approvals().get(id)`        -> `GET    /@<slug>/pending-approvals/<id>`
//!   - `approvals().approve(id, ..)`-> `POST   /@<slug>/pending-approvals/<id>/approve`
//!   - `approvals().reject(id, ..)` -> `POST   /@<slug>/pending-approvals/<id>/reject`
//!   - `approvals().bulk(..)`       -> `POST   /@<slug>/pending-approvals/bulk`
//!   - `approval_policies().list()` -> `GET    /@<slug>/approval-policies`
//!   - `approval_policies().create(..)` -> `POST /@<slug>/approval-policies`
//!   - `approval_policies().delete(id)` -> `DELETE /@<slug>/approval-policies/<id>`
//!
//! `approvals().list` is not paginated: it returns the newest `limit` changes
//! (1 to 200, default 50). Project admins and auditors see the whole queue;
//! other members see only the changes they proposed.

use crate::envelope::Response;
use crate::http::{request, HttpClientConfig};
use crate::kerror::KError;
use crate::stream::require_project;
use crate::url_grammar::{enc, project_path};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// A change waiting for (or past) a decision. `status` is `pending_approval`,
/// `approved`, `rejected`, `expired` or `redeemed`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingChange {
    /// `apr_<base62>`.
    pub id: String,
    pub status: String,
    pub collection: String,
    /// `insert`, `update` or `delete`.
    pub operation: String,
    pub maker_id: String,
    /// The proposed record for a data write; for other kinds of change, a
    /// description of the action.
    #[serde(default)]
    pub proposed_payload: Value,
    #[serde(default)]
    pub target_record_id: Option<String>,
    #[serde(default)]
    pub approver_id: Option<String>,
    #[serde(default)]
    pub decided_at: Option<String>,
    #[serde(default)]
    pub decision_reason: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// Options of `approvals().list`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ListApprovalsOptions<'a> {
    /// Only changes in this status; `None` for all.
    pub status: Option<&'a str>,
    /// 1 to 200; the gateway defaults to 50.
    pub limit: Option<u32>,
}

/// Input of `approvals().approve`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApproveInput<'a> {
    /// Optional note stored with the decision.
    pub reason: Option<&'a str>,
}

/// Input of `approvals().reject`.
#[derive(Debug, Clone, Copy)]
pub struct RejectInput<'a> {
    /// Required, non-empty.
    pub reason: &'a str,
}

/// Input of `approvals().bulk`.
#[derive(Debug, Clone, Copy)]
pub struct BulkApprovalsInput<'a> {
    /// `approve` or `reject`.
    pub action: &'a str,
    /// A predicate object over each pending change, for example
    /// `{"eq": ["change.table", "orders"]}`.
    pub criteria: &'a Value,
    /// Required when `action` is `reject`.
    pub reason: Option<&'a str>,
}

/// Result of `approvals().bulk`. Rows that fail are skipped silently and left
/// out of `affected_ids`: compare it with the set you meant to decide.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkApprovalsResult {
    pub attestation_id: String,
    pub action: String,
    pub affected_count: u64,
    #[serde(default)]
    pub affected_ids: Vec<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// A rule: writes to `collection` with `operation` that match `predicate` get `action`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalPolicy {
    /// `apo_<base62>`.
    pub id: String,
    pub collection: String,
    pub operation: String,
    #[serde(default)]
    pub predicate: Value,
    /// `auto_approve` or `queue`.
    pub action: String,
    #[serde(default)]
    pub authored_by: String,
    #[serde(default)]
    pub created_at: String,
}

/// Input of `approval_policies().create`.
#[derive(Debug, Clone, Copy)]
pub struct CreateApprovalPolicyInput<'a> {
    /// `^[a-zA-Z_][a-zA-Z0-9_]{0,62}$`.
    pub collection: &'a str,
    /// `insert`, `update` or `delete`.
    pub operation: &'a str,
    /// `auto_approve` or `queue`.
    pub action: &'a str,
    /// A predicate object.
    pub predicate: &'a Value,
}

/// Result of `approval_policies().delete`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApprovalPolicyDeleted {
    pub id: String,
}

fn invalid(message: &str) -> KError {
    KError::new("K9O_INVALID_FIELD_VALUE", message)
}

pub(crate) fn check_limit(limit: Option<u32>) -> Result<(), KError> {
    match limit {
        Some(n) if !(1..=200).contains(&n) => {
            Err(invalid("limit must be a whole number from 1 to 200").with_context("limit", n))
        }
        _ => Ok(()),
    }
}

fn require_id(id: &str, what: &str) -> Result<(), KError> {
    if id.is_empty() {
        return Err(invalid(&format!("{what} id must be a non-empty string")));
    }
    Ok(())
}

fn require_reason(reason: Option<&str>) -> Result<&str, KError> {
    match reason {
        Some(r) if !r.trim().is_empty() => Ok(r),
        _ => Err(invalid("reason is required and must be a non-empty string")),
    }
}

fn require_predicate<'v>(value: &'v Value, field: &str) -> Result<&'v Value, KError> {
    if value.is_object() {
        Ok(value)
    } else {
        Err(invalid(&format!("{field} must be a predicate object")))
    }
}

fn valid_collection(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 63 {
        return false;
    }
    let first = bytes[0];
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// Approvals namespace: `client.approvals()`.
pub struct ApprovalsNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl ApprovalsNamespace<'_> {
    /// List pending changes, newest first. Not paginated: only `limit` caps the result.
    pub async fn list(
        &self,
        opts: ListApprovalsOptions<'_>,
    ) -> Result<Response<Vec<PendingChange>>, KError> {
        check_limit(opts.limit)?;
        let slug = require_project(self.config, "approvals")?;
        let mut q: Vec<(String, String)> = Vec::new();
        if let Some(s) = opts.status {
            q.push(("status".into(), s.into()));
        }
        if let Some(n) = opts.limit {
            q.push(("limit".into(), n.to_string()));
        }
        request::<Vec<PendingChange>, ()>(
            self.config,
            Method::GET,
            &project_path(slug, "pending-approvals"),
            None,
            Some(&q),
        )
        .await
    }

    /// One pending change. Members who are not queue admins can read only their own.
    pub async fn get(&self, id: &str) -> Result<Response<PendingChange>, KError> {
        require_id(id, "approval")?;
        let slug = require_project(self.config, "approvals")?;
        request::<PendingChange, ()>(
            self.config,
            Method::GET,
            &project_path(slug, &format!("pending-approvals/{}", enc(id))),
            None,
            None,
        )
        .await
    }

    /// Approve a pending change. Read `status` on the answer: `pending_approval`
    /// means the approval was recorded but more approvers are needed.
    pub async fn approve(
        &self,
        id: &str,
        input: ApproveInput<'_>,
    ) -> Result<Response<PendingChange>, KError> {
        require_id(id, "approval")?;
        let slug = require_project(self.config, "approvals")?;
        let body = match input.reason {
            Some(r) => json!({ "reason": r }),
            None => json!({}),
        };
        request::<PendingChange, Value>(
            self.config,
            Method::POST,
            &project_path(slug, &format!("pending-approvals/{}/approve", enc(id))),
            Some(&body),
            None,
        )
        .await
    }

    /// Reject a pending change. `reason` is required.
    pub async fn reject(
        &self,
        id: &str,
        input: RejectInput<'_>,
    ) -> Result<Response<PendingChange>, KError> {
        let reason = require_reason(Some(input.reason))?;
        require_id(id, "approval")?;
        let slug = require_project(self.config, "approvals")?;
        request::<PendingChange, Value>(
            self.config,
            Method::POST,
            &project_path(slug, &format!("pending-approvals/{}/reject", enc(id))),
            Some(&json!({ "reason": reason })),
            None,
        )
        .await
    }

    /// Approve or reject every pending change matching `criteria`. Rows that
    /// fail are skipped without a report, so compare `affected_ids` with what
    /// you intended. It considers the newest 10000 pending changes.
    pub async fn bulk(
        &self,
        input: BulkApprovalsInput<'_>,
    ) -> Result<Response<BulkApprovalsResult>, KError> {
        if input.action != "approve" && input.action != "reject" {
            return Err(invalid("action must be \"approve\" or \"reject\""));
        }
        let criteria = require_predicate(input.criteria, "criteria")?;
        let mut body = Map::new();
        body.insert("action".into(), json!(input.action));
        body.insert("criteria".into(), criteria.clone());
        if input.action == "reject" {
            body.insert("reason".into(), json!(require_reason(input.reason)?));
        } else if let Some(r) = input.reason {
            body.insert("reason".into(), json!(r));
        }
        let slug = require_project(self.config, "approvals")?;
        request::<BulkApprovalsResult, Value>(
            self.config,
            Method::POST,
            &project_path(slug, "pending-approvals/bulk"),
            Some(&Value::Object(body)),
            None,
        )
        .await
    }
}

/// Approval policies namespace: `client.approval_policies()`.
pub struct ApprovalPoliciesNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl ApprovalPoliciesNamespace<'_> {
    /// All policies, oldest first. The first matching policy for a write decides.
    pub async fn list(&self) -> Result<Response<Vec<ApprovalPolicy>>, KError> {
        let slug = require_project(self.config, "approval_policies")?;
        request::<Vec<ApprovalPolicy>, ()>(
            self.config,
            Method::GET,
            &project_path(slug, "approval-policies"),
            None,
            None,
        )
        .await
    }

    /// Create a policy. Identical policies are not de-duplicated.
    pub async fn create(
        &self,
        input: CreateApprovalPolicyInput<'_>,
    ) -> Result<Response<ApprovalPolicy>, KError> {
        if !valid_collection(input.collection) {
            return Err(
                invalid("collection must match ^[a-zA-Z_][a-zA-Z0-9_]{0,62}$")
                    .with_context("collection", input.collection),
            );
        }
        if !matches!(input.operation, "insert" | "update" | "delete") {
            return Err(invalid(
                "operation must be \"insert\", \"update\" or \"delete\"",
            ));
        }
        if !matches!(input.action, "auto_approve" | "queue") {
            return Err(invalid("action must be \"auto_approve\" or \"queue\""));
        }
        let predicate = require_predicate(input.predicate, "predicate")?;
        let slug = require_project(self.config, "approval_policies")?;
        let body = json!({
            "collection": input.collection,
            "operation": input.operation,
            "action": input.action,
            "predicate": predicate,
        });
        request::<ApprovalPolicy, Value>(
            self.config,
            Method::POST,
            &project_path(slug, "approval-policies"),
            Some(&body),
            None,
        )
        .await
    }

    /// Delete a policy; answers `{ id }`.
    pub async fn delete(&self, id: &str) -> Result<Response<ApprovalPolicyDeleted>, KError> {
        require_id(id, "policy")?;
        let slug = require_project(self.config, "approval_policies")?;
        request::<ApprovalPolicyDeleted, ()>(
            self.config,
            Method::DELETE,
            &project_path(slug, &format!("approval-policies/{}", enc(id))),
            None,
            None,
        )
        .await
    }
}
