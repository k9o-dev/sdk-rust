//! Function lifecycle namespace — `client.functions()`.

use crate::envelope::Response;
use crate::http::{request, HttpClientConfig};
use crate::kerror::KError;
use crate::stream::require_project;
use crate::url_grammar::project_path;
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
    pub status: String,
    pub tier: String,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// Deploy input.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployFunctionInput<'a> {
    pub name: &'a str,
    pub runtime: &'a str,
    pub code: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
}

/// Functions namespace — `client.functions()`.
pub struct FunctionsNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl<'a> FunctionsNamespace<'a> {
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

    pub async fn list(
        &self,
        status: Option<&str>,
    ) -> Result<Response<Vec<FunctionRecord>>, KError> {
        let slug = require_project(self.config, "functions")?;
        let q: Vec<(String, String)> = status
            .map(|s| vec![("status".into(), s.into())])
            .unwrap_or_default();
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
            &project_path(slug, &format!("functions/{}", name)),
            None,
            None,
        )
        .await
    }

    pub async fn delete(&self, name: &str) -> Result<Response<Value>, KError> {
        let slug = require_project(self.config, "functions")?;
        request::<Value, ()>(
            self.config,
            Method::DELETE,
            &project_path(slug, &format!("functions/{}", name)),
            None,
            None,
        )
        .await
    }

    /// Invoke a function; payload is passed to the runtime verbatim.
    pub async fn invoke(&self, name: &str, payload: &Value) -> Result<Response<Value>, KError> {
        let slug = require_project(self.config, "functions")?;
        request::<Value, _>(
            self.config,
            Method::POST,
            &project_path(slug, &format!("functions/{}/invoke", name)),
            Some(payload),
            None,
        )
        .await
    }
}
