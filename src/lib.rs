//! # k9o — Kilimanjaro platform SDK for Rust
//!
//! Idiomatic Rust SDK: `snake_case` methods +
//! `Result<Response<T>, KError>` error delivery + `Client::new(ClientOpts{...})`
//! constructor + reqwest async transport.
//!
//! ## Canonical usage
//!
//! ```ignore
//! use k9o::{Client, ClientOpts, KError};
//! use serde::Deserialize;
//!
//! #[derive(Debug, Deserialize)]
//! struct Post { id: String, title: String }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), KError> {
//!     let k9o = Client::new(ClientOpts {
//!         base_url: "https://acme.api.k9o.io".into(),
//!         api_key: std::env::var("K9O_API_KEY").ok(),
//!         org_slug: Some("acme".into()),
//!         project_slug: Some("member-portal".into()),
//!         ..Default::default()
//!     })?;
//!
//!     match k9o.from("posts").select().limit(10).execute::<Post>().await {
//!         Ok(resp) => {
//!             for row in resp.data { println!("{:?}", row); }
//!             if let Some(cursor) = resp.meta.cursor {
//!                 println!("next cursor: {:?}", cursor.next);
//!             }
//!         }
//!         Err(e) => {
//!             if e.code == "K9O_TIER_CAP_HIT" {
//!                 eprintln!("[{}] {}: {:?}", e.code, e.message, e.recommendation);
//!             }
//!             return Err(e);
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```

pub mod ai;
pub mod audit;
pub mod client;
pub mod compute;
pub mod data;
pub mod envelope;
pub mod functions;
pub mod gate;
pub mod kerror;
pub mod kibo;
pub mod me;
pub mod migrations;
pub mod org;
pub mod orgs;
pub mod policies;
pub mod projects;
pub mod realtime;
pub mod storage;
pub mod studio;
pub mod url_grammar;

mod http;
mod region;
mod stream;

// Public re-exports
pub use client::{Client, ClientOpts};
pub use data::{DeleteBuilder, FromBuilder, SelectBuilder, UpdateBuilder};
pub use envelope::{
    Envelope, Meta, MetaContext, MetaCursor, MetaIdempotency, MetaSubscription, MetaTiming,
    MetaWarning, Response,
};
pub use gate::{GateNamespace, RefreshTokenInput, SignInInput, SignOutInput, SignUpInput};
pub use http::{Backoff, RetryConfig};
pub use kerror::KError;
pub use projects::{
    CreateProjectInput, Project, ProjectsNamespace, ShowroomLocale, UpdateProjectInput,
};
pub use url_grammar::{
    org_path, parse_api_host, platform_path, project_path, resolve_base_url, validate_base_url,
    validate_slug, ApiHost, DEFAULT_BASE_URL,
};

pub use ai::{AiNamespace, EmbedInput, EmbedResult, EmbedText};
pub use compute::{
    ComputeNamespace, LlmInvokeInput, LlmInvokeParams, LlmModel, LlmResponse, LlmStreamChunk,
    LlmUsage, LlmsNamespace, LLM_MODEL_IDS,
};
pub use org::{
    CreateIdpInput, GrantRoleInput, OrgAuditQueryOpts, OrgDefaultLlm, OrgDefaultLlmSet,
    OrgNamespace, SetSmtpInput, UpdateIdpInput,
};
pub use realtime::{topic as realtime_topic, RealtimeEvent, RealtimeNamespace, Subscription};
pub use storage::{
    BucketHandle, CreateBucketOptions, DeletedBucket, RemovedObject, SignedUrl, SignedUrlOptions,
    StorageBucket, StorageNamespace, StorageObject, UpdateBucketOptions, UploadOptions,
};
pub use stream::SseStream;
pub use studio::{
    Dashboard, DashboardsNamespace, StudioNamespace, Workflow, WorkflowRun, WorkflowRunsNamespace,
    WorkflowsNamespace,
};

pub use audit::{AuditEvent, AuditNamespace, AuditQueryOpts};
pub use functions::{
    ArtifactUpload, DeployFunctionInput, FunctionRecord, FunctionsNamespace, InvokeDelegation,
    InvokeOptions, ListFunctionsOptions,
};
pub use me::{ApiTokenScopeManifest, CreateApiTokenInput, MeNamespace};
pub use migrations::{
    ActionRule, AddColumnOp, AlterTableOp, AppliedMigration, CollectionActions, CollectionPolicy,
    ColumnEncryption, ColumnReferences, ColumnSpec, CreateIndexOp, CreateTableOp, DropColumnOp,
    DropIndexOp, IndexSpec, Migration, MigrationPolicy, MigrationStorage, MigrationsNamespace,
    Operation, PendingMigration, RelationshipSpec, StorageBucketActions, StorageBucketPolicy,
};
pub use orgs::{Org, OrgsNamespace};
pub use policies::{
    CreatePolicyInput, PoliciesNamespace, PolicyBundle, PolicyDiagnostic, UpdatePolicyInput,
    ValidatePolicyInput, ValidatePolicyResult,
};

// Kibo vault-secrets namespace (ported from the `packages/kibo` TS reference).
pub use kibo::{
    AttestationTier, CreateSecretInput, KiboNamespace, ListSecretsInput, ListSecretsResult,
    MoveSecretResult, RevealChallenge, RevealSecretResult, RotateSecretResult, SecretSummary,
    SecretVisibility, SecretsNamespace, UpdateSecretInput, UpdateSecretResult, VaultNamespace,
};
