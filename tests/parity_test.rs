//! Full-coverage parity tests for the namespaces bringing Rust to TS
//! surface parity: migrations / functions / policies / audit / orgs / me /
//! full org. Routes verified against a wiremock gateway.

use k9o::{
    AuditQueryOpts, Client, ClientOpts, ColumnSpec, CreateTableOp, DeployFunctionInput, Migration,
    Operation, SetSmtpInput,
};
use serde_json::{json, Map};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn project_client(server: &MockServer) -> Client {
    Client::new(ClientOpts {
        base_url: Some(server.uri()),
        project_slug: Some("portal".into()),
        ..Default::default()
    })
    .unwrap()
}

fn org_client(server: &MockServer) -> Client {
    Client::new(ClientOpts {
        base_url: Some(server.uri()),
        ..Default::default()
    })
    .unwrap()
}

async fn mock_ok(server: &MockServer, m: &str, p: &str, body: serde_json::Value) {
    Mock::given(method(m))
        .and(path(p))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn migrations_apply_and_pending() {
    let server = MockServer::start().await;
    mock_ok(
        &server,
        "POST",
        "/@portal/migrations/apply",
        json!({"data":{"applied":[]},"meta":{}}),
    )
    .await;
    mock_ok(
        &server,
        "POST",
        "/@portal/migrations/pending/pend_x/apply",
        json!({"data":{"version":1,"slug":"init","checksum":"c","appliedAt":"t"},"meta":{}}),
    )
    .await;
    let c = project_client(&server);
    c.migrations()
        .apply(&[Migration {
            version: 1,
            slug: "init".into(),
            up: vec![Operation::CreateTable(CreateTableOp {
                name: "posts".into(),
                columns: vec![ColumnSpec {
                    name: "id".into(),
                    r#type: "uuid".into(),
                    primary_key: Some(true),
                    not_null: None,
                    unique: None,
                    default: None,
                    references: None,
                    encrypted: None,
                    blind_indexed: None,
                }],
                indexes: None,
            })],
            down: vec![Operation::DropTable("posts".into())],
            policy: None,
        }])
        .await
        .unwrap();
    let applied = c.migrations().apply_pending("pend_x").await.unwrap();
    assert_eq!(applied.data.version, 1);
}

#[tokio::test]
async fn functions_deploy_and_invoke() {
    let server = MockServer::start().await;
    mock_ok(&server, "POST", "/@portal/functions",
        json!({"data":{"id":"fn_x","name":"greet","runtime":"bun","status":"deploying","tier":"low","createdAt":"t"},"meta":{}})).await;
    mock_ok(
        &server,
        "POST",
        "/@portal/functions/greet/invoke",
        json!({"data":{"ok":true},"meta":{}}),
    )
    .await;
    let c = project_client(&server);
    let f = c
        .functions()
        .deploy(DeployFunctionInput {
            name: "greet",
            runtime: "bun",
            code: "...",
            tier: None,
            config: None,
        })
        .await
        .unwrap();
    assert_eq!(f.data.name, "greet");
    c.functions()
        .invoke("greet", &json!({"name":"World"}))
        .await
        .unwrap();
}

#[tokio::test]
async fn policies_create_and_publish() {
    let server = MockServer::start().await;
    let bundle = json!({"data":{"id":"pol_x","name":"rls","version":1,"status":"draft","rules":{},"createdAt":"t","createdBy":"u"},"meta":{}});
    mock_ok(&server, "POST", "/@portal/policies", bundle.clone()).await;
    mock_ok(&server, "POST", "/@portal/policies/pol_x/publish", bundle).await;
    let c = project_client(&server);
    let mut rules = Map::new();
    rules.insert("a".into(), json!(1));
    let created = c.policies().create("rls", &rules).await.unwrap();
    assert_eq!(created.data.id, "pol_x");
    c.policies().publish("pol_x").await.unwrap();
}

#[tokio::test]
async fn audit_query() {
    let server = MockServer::start().await;
    mock_ok(
        &server,
        "GET",
        "/@portal/audit-log",
        json!({"data":[],"meta":{}}),
    )
    .await;
    let c = project_client(&server);
    let opts = AuditQueryOpts {
        event_type: Some("data.write.access.insert"),
        limit: Some(50),
        ..Default::default()
    };
    let resp = c.audit().query(opts).await.unwrap();
    assert!(resp.data.is_empty());
}

#[tokio::test]
async fn orgs_collection() {
    let server = MockServer::start().await;
    mock_ok(&server, "GET", "/orgs", json!({"data":[],"meta":{}})).await;
    mock_ok(&server, "POST", "/orgs/org_x/restore",
        json!({"data":{"id":"org_x","slug":"acme","name":"Acme","status":"active","createdAt":"t"},"meta":{}})).await;
    let c = org_client(&server);
    c.orgs().list().await.unwrap();
    let restored = c.orgs().restore("org_x").await.unwrap();
    assert_eq!(restored.data.slug, "acme");
}

#[tokio::test]
async fn me_namespaces() {
    let server = MockServer::start().await;
    mock_ok(
        &server,
        "GET",
        "/me",
        json!({"data":{"id":"u_x"},"meta":{}}),
    )
    .await;
    mock_ok(
        &server,
        "DELETE",
        "/me/sessions",
        json!({"data":{"revoked":3},"meta":{}}),
    )
    .await;
    mock_ok(
        &server,
        "POST",
        "/me/api-tokens",
        json!({"data":{"token":"k9o_x"},"meta":{}}),
    )
    .await;
    let c = org_client(&server);
    c.me().profile().get().await.unwrap();
    c.me().sessions().revoke_all().await.unwrap();
    c.me().api_tokens().create("ci", None, None).await.unwrap();
}

#[tokio::test]
async fn org_full_surfaces() {
    let server = MockServer::start().await;
    mock_ok(&server, "POST", "/members", json!({"data":{},"meta":{}})).await;
    mock_ok(
        &server,
        "POST",
        "/org-role-grants",
        json!({"data":{},"meta":{}}),
    )
    .await;
    mock_ok(&server, "PATCH", "/org", json!({"data":{},"meta":{}})).await;
    mock_ok(
        &server,
        "GET",
        "/org/default-llm",
        json!({"data":"mistral-7b","meta":{}}),
    )
    .await;
    let c = org_client(&server);
    c.org()
        .members()
        .invite("x@y.com", "ORG_MEMBER")
        .await
        .unwrap();
    c.org()
        .gate()
        .roles()
        .grant("u_a", "ORG_ADMIN")
        .await
        .unwrap();
    c.org()
        .smtp()
        .set(&SetSmtpInput {
            host: "smtp.x.com",
            port: 587,
            username: "u",
            password: "p",
            from_email: "n@x.com",
            from_name: "X",
            encryption: "starttls",
            reply_to_email: None,
        })
        .await
        .unwrap();
    let llm = c.org().default_llm_get().await.unwrap();
    assert_eq!(llm.data.as_deref(), Some("mistral-7b"));
}
