//! Gate namespace — `client.gate.*`.

use crate::envelope::Response;
use crate::http::{request, HttpClientConfig};
use crate::kerror::KError;
use crate::url_grammar::project_path;
use reqwest::Method;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Default, Serialize)]
pub struct SignOutInput<'a> {
    #[serde(rename = "refreshToken", skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub struct RefreshTokenInput<'a> {
    #[serde(rename = "refreshToken")]
    pub refresh_token: &'a str,
}

/// Hosted sign-in coordinates returned by `GET /@<slug>/gate/hosted`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HostedGateInfo {
    /// Absolute hosted sign-in URL (verified custom domain, else the project's Gate host).
    #[serde(rename = "signInUrl")]
    pub sign_in_url: String,
    /// WebAuthn relying-party id.
    #[serde(rename = "rpId")]
    pub rp_id: String,
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

    pub async fn verify_session(&self) -> Result<Response<Value>, KError> {
        let slug = self.require_project()?;
        request::<Value, ()>(
            self.config,
            Method::GET,
            &project_path(slug, "gate/verify"),
            None,
            None,
        )
        .await
    }

    /// Hosted sign-in URL + WebAuthn rpId for this project. Unauthenticated:
    /// sends no `Authorization` header (even with an `api_key` configured)
    /// and no cookie. An unknown/deleted/non-Gate project answers a uniform
    /// 404 `K9O_PROJECT_NOT_FOUND`.
    pub async fn hosted(&self) -> Result<Response<HostedGateInfo>, KError> {
        let slug = self.require_project()?;
        let mut cfg = self.config.clone();
        cfg.api_key = None;
        cfg.default_headers.retain(|k, _| {
            let k = k.to_ascii_lowercase();
            k != "authorization" && k != "cookie"
        });
        request::<HostedGateInfo, ()>(
            &cfg,
            Method::GET,
            &project_path(slug, "gate/hosted"),
            None,
            None,
        )
        .await
    }
}
