//! Policy bundle lifecycle namespace — `client.policies()`.

use crate::envelope::Response;
use crate::http::{request, HttpClientConfig};
use crate::kerror::KError;
use crate::stream::require_project;
use crate::url_grammar::project_path;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Policy bundle record.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyBundle {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub status: String,
    pub rules: Value,
    pub created_at: String,
    #[serde(default)]
    pub published_at: Option<String>,
    pub created_by: String,
}

#[derive(Serialize)]
struct CreateBody<'a> {
    name: &'a str,
    rules: &'a Map<String, Value>,
}

#[derive(Serialize)]
struct ValidateBody<'a> {
    rules: &'a Map<String, Value>,
}

/// Policies namespace — `client.policies()`.
pub struct PoliciesNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl<'a> PoliciesNamespace<'a> {
    /// Read the currently-active policy bundle (None data when none active).
    pub async fn current(&self) -> Result<Response<Option<PolicyBundle>>, KError> {
        let slug = require_project(self.config, "policies")?;
        request::<Option<PolicyBundle>, ()>(
            self.config,
            Method::GET,
            &project_path(slug, "policy"),
            None,
            None,
        )
        .await
    }

    pub async fn list(&self, status: Option<&str>) -> Result<Response<Vec<PolicyBundle>>, KError> {
        let slug = require_project(self.config, "policies")?;
        let q: Vec<(String, String)> = status
            .map(|s| vec![("status".into(), s.into())])
            .unwrap_or_default();
        request::<Vec<PolicyBundle>, ()>(
            self.config,
            Method::GET,
            &project_path(slug, "policies"),
            None,
            Some(&q),
        )
        .await
    }

    pub async fn create(
        &self,
        name: &str,
        rules: &Map<String, Value>,
    ) -> Result<Response<PolicyBundle>, KError> {
        let slug = require_project(self.config, "policies")?;
        request::<PolicyBundle, _>(
            self.config,
            Method::POST,
            &project_path(slug, "policies"),
            Some(&CreateBody { name, rules }),
            None,
        )
        .await
    }

    /// Validate a rules bundle without persisting it.
    pub async fn validate(&self, rules: &Map<String, Value>) -> Result<Response<Value>, KError> {
        let slug = require_project(self.config, "policies")?;
        request::<Value, _>(
            self.config,
            Method::POST,
            &project_path(slug, "policies/validate"),
            Some(&ValidateBody { rules }),
            None,
        )
        .await
    }

    pub async fn get(&self, bundle_id: &str) -> Result<Response<PolicyBundle>, KError> {
        let slug = require_project(self.config, "policies")?;
        request::<PolicyBundle, ()>(
            self.config,
            Method::GET,
            &project_path(slug, &format!("policies/{}", bundle_id)),
            None,
            None,
        )
        .await
    }

    pub async fn update(
        &self,
        bundle_id: &str,
        patch: &Value,
    ) -> Result<Response<PolicyBundle>, KError> {
        let slug = require_project(self.config, "policies")?;
        request::<PolicyBundle, _>(
            self.config,
            Method::PATCH,
            &project_path(slug, &format!("policies/{}", bundle_id)),
            Some(patch),
            None,
        )
        .await
    }

    pub async fn delete(&self, bundle_id: &str) -> Result<Response<Value>, KError> {
        let slug = require_project(self.config, "policies")?;
        request::<Value, ()>(
            self.config,
            Method::DELETE,
            &project_path(slug, &format!("policies/{}", bundle_id)),
            None,
            None,
        )
        .await
    }

    pub async fn publish(&self, bundle_id: &str) -> Result<Response<PolicyBundle>, KError> {
        let slug = require_project(self.config, "policies")?;
        request::<PolicyBundle, ()>(
            self.config,
            Method::POST,
            &project_path(slug, &format!("policies/{}/publish", bundle_id)),
            None,
            None,
        )
        .await
    }
}
