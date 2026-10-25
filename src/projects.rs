//! Project lifecycle namespace — `client.projects.*`.

use crate::envelope::Response;
use crate::http::{request, HttpClientConfig};
use crate::kerror::KError;
use crate::url_grammar::{org_path, project_path, validate_slug};
use reqwest::Method;
use serde::{Deserialize, Serialize};

/// Project record. Rust idiomatic shape (snake_case via serde
/// rename on wire camelCase).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[serde(rename = "custodyMode")]
    pub custody_mode: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<String>,
    #[serde(rename = "orgId", default)]
    pub org_id: String,
}

#[derive(Debug, Serialize)]
pub struct CreateProjectInput<'a> {
    pub slug: &'a str,
    pub name: &'a str,
    #[serde(rename = "custodyMode", skip_serializing_if = "Option::is_none")]
    pub custody_mode: Option<&'a str>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateProjectInput<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<&'a str>,
}

pub struct ProjectsNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl<'a> ProjectsNamespace<'a> {
    /// List projects accessible to the caller.
    pub async fn list(&self) -> Result<Response<Vec<Project>>, KError> {
        request::<Vec<Project>, ()>(self.config, Method::GET, &org_path("projects"), None, None).await
    }

    /// Create a new project under the caller's active org.
    pub async fn create(&self, input: CreateProjectInput<'_>) -> Result<Response<Project>, KError> {
        validate_slug(input.slug, "project")?;
        request::<Project, _>(self.config, Method::POST, &org_path("projects"), Some(&input), None).await
    }

    /// Get a project by slug.
    pub async fn get(&self, slug: &str) -> Result<Response<Project>, KError> {
        validate_slug(slug, "project")?;
        request::<Project, ()>(self.config, Method::GET, &project_path(slug, "project"), None, None).await
    }

    /// Update a project's mutable fields.
    pub async fn update(&self, slug: &str, patch: UpdateProjectInput<'_>) -> Result<Response<Project>, KError> {
        validate_slug(slug, "project")?;
        request::<Project, _>(self.config, Method::PATCH, &project_path(slug, "project"), Some(&patch), None).await
    }
}
