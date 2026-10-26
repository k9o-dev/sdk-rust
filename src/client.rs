//! `Client::new(ClientOpts{...})` factory.
//!
//! Canonical usage:
//!
//! ```ignore
//! use k9o::{Client, ClientOpts};
//!
//! let k9o = Client::new(ClientOpts {
//!     // base_url omitted → defaults to https://api.k9o.io, then derives
//!     // https://acme.api.k9o.io from org_slug.
//!     api_key: Some(std::env::var("K9O_API_KEY").unwrap()),
//!     org_slug: Some("acme".into()),
//!     project_slug: Some("member-portal".into()),
//!     ..Default::default()
//! })?;
//!
//! let resp = k9o.from("posts").select().eq("status", "published").limit(10).execute::<Post>().await?;
//! for row in resp.data { println!("{:?}", row); }
//! ```

use crate::ai::AiNamespace;
use crate::approvals::{ApprovalPoliciesNamespace, ApprovalsNamespace};
use crate::audit::AuditNamespace;
use crate::compute::ComputeNamespace;
use crate::data::FromBuilder;
use crate::functions::FunctionsNamespace;
use crate::gate::GateNamespace;
use crate::http::{HttpClientConfig, RetryConfig};
use crate::kerror::KError;
use crate::kibo::KiboNamespace;
use crate::me::MeNamespace;
use crate::migrations::MigrationsNamespace;
use crate::org::OrgNamespace;
use crate::orgs::OrgsNamespace;
use crate::policies::PoliciesNamespace;
use crate::projects::ProjectsNamespace;
use crate::realtime::RealtimeNamespace;
use crate::schema::SchemaNamespace;
use crate::storage::StorageNamespace;
use crate::streams::StreamsNamespace;
use crate::studio::StudioNamespace;
use crate::url_grammar::resolve_base_url;
use std::collections::HashMap;
use std::sync::Arc;

/// Options for `Client::new`.
#[derive(Debug, Default, Clone)]
pub struct ClientOpts {
    /// Gateway host root (scheme + host [+ port]) OPTIONAL
    ///: when `None`, defaults to the platform
    /// apex `https://api.k9o.io`; when the host is the bare apex and
    /// `org_slug` is set, the SDK derives `https://<org_slug>.api.k9o.io`.
    /// Any other host (explicit subdomain, self-hosted, dev) passes through.
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub org_slug: Option<String>,
    pub project_slug: Option<String>,
    pub default_headers: HashMap<String, String>,
    /// Custom reqwest::Client (for tests, custom transports, timeouts).
    pub reqwest_client: Option<reqwest::Client>,
    /// Retry/backoff policy; None ⇒ defaults (3 retries,
    /// exponential) on idempotent methods + transient failures.
    pub retry_config: Option<RetryConfig>,
}

/// The customer-facing client returned by `Client::new`.
#[derive(Debug)]
pub struct Client {
    pub(crate) config: HttpClientConfig,
}

impl Client {
    /// Construct a client bound to the supplied options. Validates
    /// `base_url`; returns `KError(K9O_INVALID_BASE_URL)`
    /// on violation.
    pub fn new(opts: ClientOpts) -> Result<Self, KError> {
        let normalised = resolve_base_url(opts.base_url.as_deref(), opts.org_slug.as_deref())?;
        let reqwest = opts.reqwest_client.unwrap_or_else(reqwest::Client::new);
        Ok(Self {
            config: HttpClientConfig {
                base_url: normalised,
                api_key: opts.api_key,
                org_slug: opts.org_slug,
                project_slug: opts.project_slug,
                default_headers: opts.default_headers,
                reqwest: Arc::new(reqwest),
                retry: opts.retry_config.unwrap_or_default(),
                region: Arc::new(crate::region::RegionState::new()),
            },
        })
    }

    /// Data-plane DSL entry point.
    pub fn from(&self, collection: &str) -> FromBuilder<'_> {
        FromBuilder {
            config: &self.config,
            collection: collection.into(),
        }
    }

    /// Projects namespace.
    pub fn projects(&self) -> ProjectsNamespace<'_> {
        ProjectsNamespace {
            config: &self.config,
        }
    }

    /// Gate (IAK) namespace.
    pub fn gate(&self) -> GateNamespace<'_> {
        GateNamespace {
            config: &self.config,
        }
    }

    /// Studio namespace — workflows + dashboards.
    pub fn studio(&self) -> StudioNamespace<'_> {
        StudioNamespace {
            config: &self.config,
        }
    }

    /// AI namespace — platform AI primitives (`embed`).
    pub fn ai(&self) -> AiNamespace<'_> {
        AiNamespace {
            config: &self.config,
        }
    }

    /// Storage namespace — buckets and objects.
    pub fn storage(&self) -> StorageNamespace<'_> {
        StorageNamespace {
            config: &self.config,
        }
    }

    /// Compute namespace — LLMs registry.
    pub fn compute(&self) -> ComputeNamespace<'_> {
        ComputeNamespace {
            config: &self.config,
        }
    }

    /// Realtime subscriptions namespace.
    pub fn realtime(&self) -> RealtimeNamespace<'_> {
        RealtimeNamespace {
            config: &self.config,
        }
    }

    /// User data streams namespace.
    pub fn streams(&self) -> StreamsNamespace<'_> {
        StreamsNamespace {
            config: &self.config,
        }
    }

    /// Org-plane bound namespace — members/groups/smtp/audit_log/gate/default_llm.
    pub fn org(&self) -> OrgNamespace<'_> {
        OrgNamespace {
            config: &self.config,
        }
    }

    /// Org lifecycle namespace (collection-style).
    pub fn orgs(&self) -> OrgsNamespace<'_> {
        OrgsNamespace {
            config: &self.config,
        }
    }

    /// Migration lifecycle namespace
    pub fn migrations(&self) -> MigrationsNamespace<'_> {
        MigrationsNamespace {
            config: &self.config,
        }
    }

    /// Function lifecycle namespace.
    pub fn functions(&self) -> FunctionsNamespace<'_> {
        FunctionsNamespace {
            config: &self.config,
        }
    }

    /// Policy bundle lifecycle namespace.
    pub fn policies(&self) -> PoliciesNamespace<'_> {
        PoliciesNamespace {
            config: &self.config,
        }
    }

    /// Approvals namespace: the pending-change queue.
    pub fn approvals(&self) -> ApprovalsNamespace<'_> {
        ApprovalsNamespace {
            config: &self.config,
        }
    }

    /// Approval policies namespace: the rules that decide which writes are queued.
    pub fn approval_policies(&self) -> ApprovalPoliciesNamespace<'_> {
        ApprovalPoliciesNamespace {
            config: &self.config,
        }
    }

    /// Schema namespace: `schema().collections().list(..)`.
    pub fn schema(&self) -> SchemaNamespace<'_> {
        SchemaNamespace {
            config: &self.config,
        }
    }

    /// Project-plane audit log namespace
    pub fn audit(&self) -> AuditNamespace<'_> {
        AuditNamespace {
            config: &self.config,
        }
    }

    /// Operator-self namespace.
    pub fn me(&self) -> MeNamespace<'_> {
        MeNamespace {
            config: &self.config,
        }
    }

    /// Kibo (BYOK/HYOK vault) namespace — `client.kibo().vault().secrets()`
    ///
    pub fn kibo(&self) -> KiboNamespace<'_> {
        KiboNamespace {
            config: &self.config,
        }
    }

    /// Bound config (read-only access for advanced use).
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }
}
