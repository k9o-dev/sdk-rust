//! Org-plane bound namespace — `client.org()`.
//!
//! Singular bound-org namespace (distinct from `client.orgs()` collection):
//! members / groups / smtp / audit_log / gate (roles + idp_providers) +
//! default_llm. Org-plane URL grammar routes against the
//! org subdomain without the `@<projectSlug>/` prefix.

use crate::audit::AuditQueryOpts;
use crate::envelope::Response;
use crate::http::{request, HttpClientConfig};
use crate::kerror::KError;
use crate::url_grammar::org_path;
use reqwest::Method;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct SetDefaultLlmBody<'a> {
    #[serde(rename = "modelId")]
    model_id: &'a str,
}

#[derive(Serialize)]
struct InviteBody<'a> {
    email: &'a str,
    role: &'a str,
}

#[derive(Serialize)]
struct NameBody<'a> {
    name: &'a str,
}

#[derive(Serialize)]
struct GrantBody<'a> {
    #[serde(rename = "userId")]
    user_id: &'a str,
    role: &'a str,
}

/// Input for `org().smtp().set()` (password routed via Kibo enclave).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSmtpInput<'a> {
    pub host: &'a str,
    pub port: u16,
    pub username: &'a str,
    pub password: &'a str,
    pub from_email: &'a str,
    pub from_name: &'a str,
    pub encryption: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_email: Option<&'a str>,
}

#[derive(Serialize)]
struct SmtpConfigBody<'a> {
    #[serde(rename = "smtpConfig")]
    smtp_config: &'a SetSmtpInput<'a>,
}

#[derive(Serialize)]
struct SmtpUnsetBody {
    #[serde(rename = "smtpConfig")]
    smtp_config: Option<()>,
}

#[derive(Serialize)]
struct TestEmailBody<'a> {
    to: &'a str,
    #[serde(rename = "fromOverride", skip_serializing_if = "Option::is_none")]
    from_override: Option<&'a str>,
}

/// Org namespace — `client.org()`.
pub struct OrgNamespace<'a> {
    pub(crate) config: &'a HttpClientConfig,
}

impl<'a> OrgNamespace<'a> {
    pub fn members(&self) -> OrgMembers<'a> {
        OrgMembers {
            config: self.config,
        }
    }
    pub fn groups(&self) -> OrgGroups<'a> {
        OrgGroups {
            config: self.config,
        }
    }
    pub fn smtp(&self) -> OrgSmtp<'a> {
        OrgSmtp {
            config: self.config,
        }
    }
    pub fn audit_log(&self) -> OrgAuditLog<'a> {
        OrgAuditLog {
            config: self.config,
        }
    }
    pub fn gate(&self) -> OrgGate<'a> {
        OrgGate {
            config: self.config,
        }
    }

    /// Read the current org (slug + name + status + settings).
    pub async fn get(&self) -> Result<Response<Value>, KError> {
        request::<Value, ()>(self.config, Method::GET, &org_path("org"), None, None).await
    }

    /// Read the org-default LLM model id (None when unset → mistral-7b).
    pub async fn default_llm_get(&self) -> Result<Response<Option<String>>, KError> {
        request::<Option<String>, ()>(
            self.config,
            Method::GET,
            &org_path("org/default-llm"),
            None,
            None,
        )
        .await
    }

    /// Set the org-default LLM (ORG_ADMIN; tier-gated).
    pub async fn default_llm_set(&self, model_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::PATCH,
            &org_path("org/default-llm"),
            Some(&SetDefaultLlmBody { model_id }),
            None,
        )
        .await
    }
}

pub struct OrgMembers<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> OrgMembers<'a> {
    pub async fn list(&self, status: Option<&str>) -> Result<Response<Value>, KError> {
        let q: Vec<(String, String)> = status
            .map(|s| vec![("status".into(), s.into())])
            .unwrap_or_default();
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path("members"),
            None,
            Some(&q),
        )
        .await
    }
    pub async fn invite(&self, email: &str, role: &str) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::POST,
            &org_path("members"),
            Some(&InviteBody { email, role }),
            None,
        )
        .await
    }
    pub async fn get(&self, user_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path(&format!("members/{}", user_id)),
            None,
            None,
        )
        .await
    }
    pub async fn remove(&self, user_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::DELETE,
            &org_path(&format!("members/{}", user_id)),
            None,
            None,
        )
        .await
    }
}

pub struct OrgGroups<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> OrgGroups<'a> {
    pub async fn list(&self) -> Result<Response<Value>, KError> {
        request::<Value, ()>(self.config, Method::GET, &org_path("groups"), None, None).await
    }
    pub async fn create(&self, name: &str) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::POST,
            &org_path("groups"),
            Some(&NameBody { name }),
            None,
        )
        .await
    }
    pub async fn get(&self, group_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path(&format!("groups/{}", group_id)),
            None,
            None,
        )
        .await
    }
    pub async fn delete(&self, group_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::DELETE,
            &org_path(&format!("groups/{}", group_id)),
            None,
            None,
        )
        .await
    }
}

pub struct OrgAuditLog<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> OrgAuditLog<'a> {
    pub async fn query(&self, opts: AuditQueryOpts<'_>) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path("audit-log"),
            None,
            Some(&opts.to_query()),
        )
        .await
    }
}

pub struct OrgGate<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> OrgGate<'a> {
    pub fn roles(&self) -> OrgRoles<'a> {
        OrgRoles {
            config: self.config,
        }
    }
    pub fn idp_providers(&self) -> OrgIdpProviders<'a> {
        OrgIdpProviders {
            config: self.config,
        }
    }
}

pub struct OrgRoles<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> OrgRoles<'a> {
    pub async fn list(
        &self,
        user_id: Option<&str>,
        role: Option<&str>,
    ) -> Result<Response<Value>, KError> {
        let mut q: Vec<(String, String)> = Vec::new();
        if let Some(u) = user_id {
            q.push(("userId".into(), u.into()));
        }
        if let Some(r) = role {
            q.push(("role".into(), r.into()));
        }
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path("org-role-grants"),
            None,
            Some(&q),
        )
        .await
    }
    pub async fn grant(&self, user_id: &str, role: &str) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::POST,
            &org_path("org-role-grants"),
            Some(&GrantBody { user_id, role }),
            None,
        )
        .await
    }
    pub async fn get(&self, grant_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path(&format!("org-role-grants/{}", grant_id)),
            None,
            None,
        )
        .await
    }
    pub async fn revoke(&self, grant_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::DELETE,
            &org_path(&format!("org-role-grants/{}", grant_id)),
            None,
            None,
        )
        .await
    }
}

pub struct OrgIdpProviders<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> OrgIdpProviders<'a> {
    pub async fn list(&self) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path("idp-providers"),
            None,
            None,
        )
        .await
    }
    pub async fn create(&self, body: &Value) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::POST,
            &org_path("idp-providers"),
            Some(body),
            None,
        )
        .await
    }
    pub async fn get(&self, idp_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::GET,
            &org_path(&format!("idp-providers/{}", idp_id)),
            None,
            None,
        )
        .await
    }
    pub async fn configure(&self, idp_id: &str, patch: &Value) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::PATCH,
            &org_path(&format!("idp-providers/{}", idp_id)),
            Some(patch),
            None,
        )
        .await
    }
    pub async fn delete(&self, idp_id: &str) -> Result<Response<Value>, KError> {
        request::<Value, ()>(
            self.config,
            Method::DELETE,
            &org_path(&format!("idp-providers/{}", idp_id)),
            None,
            None,
        )
        .await
    }
}

pub struct OrgSmtp<'a> {
    config: &'a HttpClientConfig,
}
impl<'a> OrgSmtp<'a> {
    pub async fn get(&self) -> Result<Response<Value>, KError> {
        request::<Value, ()>(self.config, Method::GET, &org_path("org/smtp"), None, None).await
    }
    pub async fn set(&self, input: &SetSmtpInput<'_>) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::PATCH,
            &org_path("org"),
            Some(&SmtpConfigBody { smtp_config: input }),
            None,
        )
        .await
    }
    pub async fn test(
        &self,
        to: &str,
        from_override: Option<&str>,
    ) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::POST,
            &org_path("org/smtp/test-email"),
            Some(&TestEmailBody { to, from_override }),
            None,
        )
        .await
    }
    pub async fn unset(&self) -> Result<Response<Value>, KError> {
        request::<Value, _>(
            self.config,
            Method::PATCH,
            &org_path("org"),
            Some(&SmtpUnsetBody { smtp_config: None }),
            None,
        )
        .await
    }
}
