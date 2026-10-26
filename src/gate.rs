//! Gate namespace — `client.gate.*`.

use crate::envelope::Response;
use crate::http::{request, HttpClientConfig};
use crate::kerror::KError;
use crate::url_grammar::project_path;
use reqwest::Method;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct SignInInput<'a> {
    pub email: &'a str,
    pub password: &'a str,
}

#[derive(Debug, Serialize)]
pub struct SignUpInput<'a> {
    pub email: &'a str,
    pub password: &'a str,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub struct SignOutInput<'a> {
    /// The refresh token of the session to end. Required: a sign-out without
    /// one is a 400.
    #[serde(rename = "refreshToken")]
    pub refresh_token: &'a str,
}

#[derive(Debug, Serialize)]
pub struct RefreshTokenInput<'a> {
    #[serde(rename = "refreshToken")]
    pub refresh_token: &'a str,
}

pub struct GateNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl<'a> GateNamespace<'a> {
    fn require_project(&self) -> Result<&str, KError> {
        self.config.project_slug.as_deref().ok_or_else(|| {
            KError::new(
                "K9O_MISSING_PROJECT_CONTEXT",
                "client.gate operations require project_slug bound on the client",
            )
        })
    }

    pub async fn sign_up(&self, input: SignUpInput<'_>) -> Result<Response<Value>, KError> {
        let slug = self.require_project()?;
        request::<Value, _>(
            self.config,
            Method::POST,
            &project_path(slug, "gate/signup"),
            Some(&input),
            None,
        )
        .await
    }

    pub async fn sign_in(&self, input: SignInInput<'_>) -> Result<Response<Value>, KError> {
        let slug = self.require_project()?;
        request::<Value, _>(
            self.config,
            Method::POST,
            &project_path(slug, "gate/signin"),
            Some(&input),
            None,
        )
        .await
    }

    pub async fn sign_out(&self, input: SignOutInput<'_>) -> Result<Response<Value>, KError> {
        let slug = self.require_project()?;
        request::<Value, _>(
            self.config,
            Method::POST,
            &project_path(slug, "gate/signout"),
            Some(&input),
            None,
        )
        .await
    }

    pub async fn whoami(&self) -> Result<Response<Value>, KError> {
        let slug = self.require_project()?;
        request::<Value, ()>(
            self.config,
            Method::GET,
            &project_path(slug, "gate/whoami"),
            None,
            None,
        )
        .await
    }

    pub async fn refresh_token(
        &self,
        input: RefreshTokenInput<'_>,
    ) -> Result<Response<Value>, KError> {
        let slug = self.require_project()?;
        request::<Value, _>(
            self.config,
            Method::POST,
            &project_path(slug, "gate/refresh"),
            Some(&input),
            None,
        )
        .await
    }
}
