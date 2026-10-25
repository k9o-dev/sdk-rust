//! P6 surface-family tests (studio / compute.llms / realtime / org
//! default-LLM). Routes verified against a wiremock gateway.

use k9o::{realtime_topic, Client, ClientOpts, LlmInvokeInput, ReplayInput, Transport};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(server: &MockServer) -> Client {
    Client::new(ClientOpts {
        base_url: Some(server.uri()),
        project_slug: Some("portal".into()),
        ..Default::default()
    })
    .unwrap()
}

#[tokio::test]
async fn workflows_crud_and_runs() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/@portal/workflows"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":[],"meta":{}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/@portal/workflows/wfl_x/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"id":"wfr_1","workflowId":"wfl_x","projectId":"p","status":"pending"},
            "meta": {}
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/@portal/workflows/wfl_x/runs/wfr_1/cancel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"id":"wfr_1","workflowId":"wfl_x","projectId":"p","status":"cancelled"},
            "meta": {}
        })))
        .mount(&server)
        .await;
    let c = client(&server);

    c.studio()
        .workflows()
        .list(Some("active"), None)
        .await
        .unwrap();
    let run = c
        .studio()
        .workflows()
        .trigger("wfl_x", &serde_json::json!({"foo":"bar"}))
        .await
        .unwrap();
    assert_eq!(run.data.status, "pending");
    let cancelled = c
        .studio()
        .workflows()
        .runs()
        .cancel("wfl_x", "wfr_1")
        .await
        .unwrap();
    assert_eq!(cancelled.data.status, "cancelled");
}

#[tokio::test]
async fn dashboards_pin() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/@portal/dashboards/dsh_x/pin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"id":"dsh_x","projectId":"p","name":"ops","status":"active","pinnedToWorkspace":true},
            "meta": {}
        })))
        .mount(&server)
        .await;
    let c = client(&server);

    let pinned = c.studio().dashboards().pin("dsh_x").await.unwrap();
    assert!(pinned.data.pinned_to_workspace);
}

#[tokio::test]
async fn llms_invoke() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/@portal/compute/llms/mistral-7b/invoke"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"completion":"hi","tokensUsed":2,"latencyMs":9},
            "meta": {}
        })))
        .mount(&server)
        .await;
    let c = client(&server);
    let resp = c
        .compute()
        .llms()
        .invoke(
            "mistral-7b",
            LlmInvokeInput {
                prompt: "hello",
                params: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(resp.data.completion, "hi");
}

#[tokio::test]
async fn llms_invoke_stream_yields_chunks() {
    let server = MockServer::start().await;
    let sse = "data: {\"delta\": \"Hel\"}\n\ndata: {\"delta\": \"lo\", \"done\": true, \"tokensUsed\": 1}\n\ndata: [DONE]\n\n";
    Mock::given(method("POST"))
        .and(path("/@portal/compute/llms/gpt-4o/invoke-stream"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse),
        )
        .mount(&server)
        .await;
    let c = client(&server);
    let mut stream = c
        .compute()
        .llms()
        .invoke_stream(
            "gpt-4o",
            LlmInvokeInput {
                prompt: "hi",
                params: None,
            },
        )
        .await
        .unwrap();
    let mut acc = String::new();
    let mut last_done = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.unwrap();
        acc.push_str(&chunk.delta);
        last_done = chunk.done;
    }
    assert_eq!(acc, "Hello");
    assert!(last_done);
}

#[tokio::test]
async fn org_default_llm_get_set() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/org/default-llm"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data":"mistral-7b","meta":{}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/org/default-llm"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data":"claude-3-5-sonnet","meta":{}})),
        )
        .mount(&server)
        .await;
    let c = Client::new(ClientOpts {
        base_url: Some(server.uri()),
        ..Default::default()
    })
    .unwrap();
    let got = c.org().default_llm_get().await.unwrap();
    assert_eq!(got.data.as_deref(), Some("mistral-7b"));
    c.org().default_llm_set("claude-3-5-sonnet").await.unwrap();
}

#[tokio::test]
async fn realtime_topic_builders() {
    assert_eq!(realtime_topic::collection("orders"), "collections.orders.*");
    assert_eq!(realtime_topic::workflow_runs(), "studio.workflows.runs.*");
    assert_eq!(
        realtime_topic::dashboard_refresh("dsh_x"),
        "studio.dashboards.refresh.dsh_x"
    );
    assert_eq!(
        realtime_topic::function_invocations("fn_x"),
        "forge.functions.invocations.fn_x"
    );
    assert_eq!(
        realtime_topic::audit_org("role_granted"),
        "audit.org.role_granted"
    );
}

#[tokio::test]
async fn realtime_replay_rejects_negative() {
    let c = Client::new(ClientOpts {
        base_url: Some("https://acme.api.k9o.io".into()),
        project_slug: Some("portal".into()),
        ..Default::default()
    })
    .unwrap();
    let e = c
        .realtime()
        .replay(ReplayInput {
            topic: "studio.workflows.runs.*",
            from_sequence: -1,
            filter: None,
            until_sequence: None,
            limit: None,
        })
        .await
        .unwrap_err();
    assert_eq!(e.code, "K9O_INVALID_FILTER");
}

#[tokio::test]
async fn realtime_subscribe_sse() {
    let server = MockServer::start().await;
    let sse = "data: {\"topic\":\"collections.orders.*\",\"type\":\"row_inserted\",\"sequence\":1,\"payload\":{},\"occurredAt\":\"t1\"}\n\ndata: {\"topic\":\"collections.orders.*\",\"type\":\"row_updated\",\"sequence\":2,\"payload\":{},\"occurredAt\":\"t2\"}\n\n";
    Mock::given(method("GET"))
        .and(path("/@portal/realtime/subscribe"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse),
        )
        .mount(&server)
        .await;
    let c = client(&server);
    let mut sub = c
        .realtime()
        .subscribe(
            &realtime_topic::collection("orders"),
            None,
            Transport::Sse,
            None,
        )
        .await
        .unwrap();
    let mut types = Vec::new();
    while let Some(ev) = sub.next().await {
        types.push(ev.unwrap().event_type);
    }
    assert_eq!(types, vec!["row_inserted", "row_updated"]);
}

#[tokio::test]
async fn realtime_subscribe_poll() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/@portal/realtime/replay"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{"topic":"studio.workflows.runs.*","type":"workflow.run.completed","sequence":5,"payload":{},"occurredAt":"t"}],
            "meta": {}
        })))
        .mount(&server)
        .await;
    let c = client(&server);
    let mut sub = c
        .realtime()
        .subscribe(
            &realtime_topic::workflow_runs(),
            None,
            Transport::Poll,
            Some(Duration::from_millis(10)),
        )
        .await
        .unwrap();
    // First buffered event arrives; then drop the subscription to stop polling.
    let ev = sub.next().await.unwrap().unwrap();
    assert_eq!(ev.event_type, "workflow.run.completed");
    drop(sub);
}
