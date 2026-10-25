//! Operator-self namespace — `client.me()`.
//! Cross-org; org-plane routes under `/me`.

use crate::audit::AuditQueryOpts;
use crate::envelope::Response;
use crate::http::{request, HttpClientConfig};
use crate::kerror::KError;
use crate::url_grammar::org_path;
use reqwest::Method;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct DisplayNameBody<'a> {
    #[serde(rename = "displayName")]
    display_name: &'a str,
}

#[derive(Serialize)]
struct CreateTokenBody<'a> {
    name: &'a str,
    #[serde(rename = "expiresInDays", skip_serializing_if = "Option::is_none")]
    expires_in_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<&'a str>,
}

#[derive(Serialize)]
struct ReadBody {
    read: bool,
}

#[derive(Serialize)]
struct FormatBody<'a> {
    format: &'a str,
}

/// Me namespace — `client.me()`. Accessors return sub-namespace handles.
pub struct MeNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl<'a> MeNamespace<'a> {
    pub fn profile(&self) -> MeProfile<'a> {
        MeProfile {
            config: self.config,
        }
    }
    pub fn sessions(&self) -> MeSessions<'a> {
        MeSessions {
            config: self.config,
        }
    }
    pub fn api_tokens(&self) -> MeApiTokens<'a> {
        MeApiTokens {
            config: self.config,
        }
    }
    pub fn notifications(&self) -> MeNotifications<'a> {
        MeNotifications {
            config: self.config,
        }
    }
    pub fn audit_log(&self) -> MeAuditLog<'a> {
        MeAuditLog {
            config: self.config,
        }
    }
    pub fn data_export(&self) -> MeDataExport<'a> {
        MeDataExport {
            config: self.config,
        }
    }
}

pub struct MeProfile<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> MeProfile<'a> {
    pub async fn get(&self) -> Result<Response<Value>, KError> {
        request::<Value, ()>(self.config, Method::GET, &org_path("me"), None, None).await
    }
    pub async fn update(&self, display_name: &str) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::PATCH,
            &org_path("me"),
            Some(&DisplayNameBody { display_name }),
            None,
        )
        .await
    }
}

pub struct MeSessions<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> MeSessions<'a> {
    pub async fn list(&self) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path("me/sessions"),
            None,
            None,
        )
        .await
    }
    pub async fn revoke(&self, session_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::DELETE,
            &org_path(&format!("me/sessions/{}", session_id)),
            None,
            None,
        )
        .await
    }
    pub async fn revoke_all(&self) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::DELETE,
            &org_path("me/sessions"),
            None,
            None,
        )
        .await
    }
}

pub struct MeApiTokens<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> MeApiTokens<'a> {
    pub async fn list(&self) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path("me/api-tokens"),
            None,
            None,
        )
        .await
    }
    pub async fn create(
        &self,
        name: &str,
        expires_in_days: Option<u32>,
        scope: Option<&str>,
    ) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::POST,
            &org_path("me/api-tokens"),
            Some(&CreateTokenBody {
                name,
                expires_in_days,
                scope,
            }),
            None,
        )
        .await
    }
    pub async fn delete(&self, token_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::DELETE,
            &org_path(&format!("me/api-tokens/{}", token_id)),
            None,
            None,
        )
        .await
    }
}

pub struct MeNotifications<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> MeNotifications<'a> {
    pub async fn list(
        &self,
        unread_only: bool,
        limit: Option<u32>,
    ) -> Result<Response<Value>, KError> {
        let mut q: Vec<(String, String)> = Vec::new();
        if unread_only {
            q.push(("unreadOnly".into(), "true".into()));
        }
        if let Some(l) = limit {
            q.push(("limit".into(), l.to_string()));
        }
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path("me/notifications"),
            None,
            Some(&q),
        )
        .await
    }
    pub async fn mark_read(&self, notification_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::PATCH,
            &org_path(&format!("me/notifications/{}", notification_id)),
            Some(&ReadBody { read: true }),
            None,
        )
        .await
    }
    pub async fn mark_all_read(&self) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::PATCH,
            &org_path("me/notifications"),
            Some(&ReadBody { read: true }),
            None,
        )
        .await
    }
}

pub struct MeAuditLog<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> MeAuditLog<'a> {
    pub async fn query(&self, opts: AuditQueryOpts<'_>) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path("me/audit-log"),
            None,
            Some(&opts.to_query()),
        )
        .await
    }
}

pub struct MeDataExport<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> MeDataExport<'a> {
    pub async fn request_export(&self, format: &str) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::POST,
            &org_path("me/data-export"),
            Some(&FormatBody { format }),
            None,
        )
        .await
    }
    pub async fn status(&self, job_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path(&format!("me/data-export/{}", job_id)),
            None,
            None,
        )
        .await
    }
}
