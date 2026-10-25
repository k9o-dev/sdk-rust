use k9o::{Client, ClientOpts};
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Debug, Deserialize, PartialEq)]
struct Post {
    id: String,
    title: String,
}

#[tokio::test]
async fn client_constructs_with_minimal_opts() {
    let c = Client::new(ClientOpts {
        base_url: Some("https://api.k9o.io".into()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(c.base_url(), "https://api.k9o.io");
}

#[tokio::test]
async fn client_rejects_path_bearing_base_url() {
    let e = Client::new(ClientOpts {
        base_url: Some("https://api.k9o.io/v2".into()),
        ..Default::default()
    })
    .unwrap_err();
    assert_eq!(e.code, "K9O_INVALID_BASE_URL");
}

#[tokio::test]
async fn projects_list_routes_correctly() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/projects"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":[],"meta":{}})),
        )
        .mount(&server)
        .await;

    let c = Client::new(ClientOpts {
        base_url: Some(server.uri()),
        org_slug: Some("acme".into()),
        ..Default::default()
    })
    .unwrap();
    let resp = c.projects().list().await.unwrap();
    assert!(resp.data.is_empty());
}

#[tokio::test]
async fn projects_create_validates_slug() {
    let c = Client::new(ClientOpts {
        base_url: Some("https://api.k9o.io".into()),
        ..Default::default()
    })
    .unwrap();
    let e = c
        .projects()
        .create(k9o::CreateProjectInput {
            slug: "Bad-Slug",
            name: "x",
            custody_mode: None,
        })
        .await
        .unwrap_err();
    assert_eq!(e.code, "K9O_INVALID_FIELD_VALUE");
}

#[tokio::test]
async fn projects_get_routes_to_project_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/@member-portal/project"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"id":"p","slug":"member-portal","name":"MP","custodyMode":"managed","createdAt":"x","orgId":"o"},
            "meta": {}
        })))
        .mount(&server)
        .await;

    let c = Client::new(ClientOpts {
        base_url: Some(server.uri()),
        ..Default::default()
    })
    .unwrap();
    let resp = c.projects().get("member-portal").await.unwrap();
    assert_eq!(resp.data.slug, "member-portal");
}

#[tokio::test]
async fn data_select_routes_correctly() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/@member-portal/data/posts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data":[{"id":"r1","title":"hi"}],
            "meta":{"cursor":{"next":"abc"}}
        })))
        .mount(&server)
        .await;

    let c = Client::new(ClientOpts {
        base_url: Some(server.uri()),
        project_slug: Some("member-portal".into()),
        ..Default::default()
    })
    .unwrap();
    let resp = c
        .from("posts")
        .select()
        .eq("status", "published")
        .gt("views", 100)
        .limit(20)
        .execute::<Post>()
        .await
        .unwrap();
    assert_eq!(resp.data.len(), 1);
    assert_eq!(resp.data[0].title, "hi");
    assert_eq!(
        resp.meta.cursor.as_ref().unwrap().next.as_deref(),
        Some("abc")
    );
}

#[tokio::test]
async fn data_select_requires_project_slug() {
    let c = Client::new(ClientOpts {
        base_url: Some("https://api.k9o.io".into()),
        ..Default::default()
    })
    .unwrap();
    let e = c
        .from("posts")
        .select()
        .execute::<Post>()
        .await
        .unwrap_err();
    assert_eq!(e.code, "K9O_MISSING_PROJECT_CONTEXT");
}

#[tokio::test]
async fn data_update_requires_id_filter() {
    let c = Client::new(ClientOpts {
        base_url: Some("https://api.k9o.io".into()),
        project_slug: Some("member-portal".into()),
        ..Default::default()
    })
    .unwrap();
    let patch = serde_json::json!({"title": "x"});
    let e = c
        .from("posts")
        .update(patch)
        .execute::<serde_json::Value>()
        .await
        .unwrap_err();
    assert_eq!(e.code, "K9O_INVALID_FIELD_VALUE");
}

#[tokio::test]
async fn error_handling_rehydrates_kerror() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/projects"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "code": "K9O_TIER_CAP_HIT",
            "message": "cap exceeded",
            "context": {"dimension": "mau", "limit": 1000},
            "recommendation": "upgrade to Ascent",
            "requestId": "req_x"
        })))
        .mount(&server)
        .await;

    let c = Client::new(ClientOpts {
        base_url: Some(server.uri()),
        org_slug: Some("acme".into()),
        ..Default::default()
    })
    .unwrap();
    let e = c.projects().list().await.unwrap_err();
    assert_eq!(e.code, "K9O_TIER_CAP_HIT");
    assert_eq!(e.status, Some(429));
    assert_eq!(e.recommendation.as_deref(), Some("upgrade to Ascent"));
    assert_eq!(e.request_id.as_deref(), Some("req_x"));
}

#[tokio::test]
async fn gate_sign_in_routes_correctly() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/@member-portal/gate/signin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data":{"user":{"id":"eu_x"},"session":{}},
            "meta":{}
        })))
        .mount(&server)
        .await;

    let c = Client::new(ClientOpts {
        base_url: Some(server.uri()),
        project_slug: Some("member-portal".into()),
        ..Default::default()
    })
    .unwrap();
    let resp = c
        .gate()
        .sign_in(k9o::SignInInput {
            email: "x@y.com",
            password: "z",
        })
        .await
        .unwrap();
    assert!(resp.data.get("user").is_some());
}

#[tokio::test]
async fn gate_requires_project_slug() {
    let c = Client::new(ClientOpts {
        base_url: Some("https://api.k9o.io".into()),
        ..Default::default()
    })
    .unwrap();
    let e = c.gate().whoami().await.unwrap_err();
    assert_eq!(e.code, "K9O_MISSING_PROJECT_CONTEXT");
}

fn hosted_client(uri: String, key: Option<&str>, slug: Option<&str>) -> Client {
    Client::new(ClientOpts {
        base_url: Some(uri),
        project_slug: slug.map(Into::into),
        api_key: key.map(Into::into),
        ..Default::default()
    })
    .unwrap()
}

#[tokio::test]
async fn gate_hosted_get_no_authorization_returns_info() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/@member-portal/gate/hosted"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data":{"signInUrl":"https://auth.example.com/signin","rpId":"example.com"},
            "meta":{}
        })))
        .mount(&server)
        .await;
    let c = hosted_client(
        server.uri(),
        Some("k9o_secret_token"),
        Some("member-portal"),
    );
    let r = c.gate().hosted().await.unwrap();
    assert_eq!(r.data.sign_in_url, "https://auth.example.com/signin");
    assert_eq!(r.data.rp_id, "example.com");
    let reqs = server.received_requests().await.unwrap();
    let hosted: Vec<_> = reqs
        .iter()
        .filter(|r| r.url.path() == "/@member-portal/gate/hosted")
        .collect();
    assert_eq!(hosted.len(), 1);
    assert_eq!(hosted[0].method.as_str(), "GET");
    assert!(
        hosted[0].headers.get("authorization").is_none(),
        "authorization leaked"
    );
    assert!(hosted[0].headers.get("cookie").is_none(), "cookie leaked");
}

#[tokio::test]
async fn gate_hosted_uniform_404_is_kerror() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/@nope/gate/hosted"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "code": "K9O_PROJECT_NOT_FOUND",
            "message": "project not found"
        })))
        .mount(&server)
        .await;
    let c = hosted_client(server.uri(), None, Some("nope"));
    let e = c.gate().hosted().await.unwrap_err();
    assert_eq!(e.code, "K9O_PROJECT_NOT_FOUND");
    assert_eq!(e.status, Some(404));
}

#[tokio::test]
async fn gate_hosted_requires_project_slug_sends_nothing() {
    let server = MockServer::start().await;
    let c = hosted_client(server.uri(), None, None);
    let e = c.gate().hosted().await.unwrap_err();
    assert_eq!(e.code, "K9O_MISSING_PROJECT_CONTEXT");
    assert!(server.received_requests().await.unwrap().is_empty());
}
