# k9o

Client for Kilimanjaro. Read and write your data, store files, manage policies and migrations,
deploy and invoke agents or functions, and publish to and subscribe to streams and realtime events.

## Contents

- [Links](#links)
- [Install](#install)
- [Quick start](#quick-start)
- [Authentication and project context](#authentication-and-project-context)
- [Running queries](#running-queries)
- [Handling errors](#handling-errors)
- [Deploying a function](#deploying-a-function)
- [Streams](#streams)
- [Approvals](#approvals)
- [Approval policies](#approval-policies)
- [Schema collections](#schema-collections)
- [Query with SQL](#query-with-sql)
- [Search](#search)
- [Generate text](#generate-text)
- [Regional hosts](#regional-hosts)
- [Appendix](#appendix): [client options](#client-options), [functions](#functions-by-namespace),
  [error fields](#error-fields), [error codes](#error-codes), [related packages](#related-packages),
  [about Kilimanjaro](#about-kilimanjaro)

## Links

- Website: <https://kilimanjaro.dev>
- Documentation / Developer Hub: <https://docs.kilimanjaro.dev>
- Platform API: <https://api.k9o.io>
- Platform Console: <https://console.k9o.io>

## Install

```toml
[dependencies]
k9o = "1.4"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
```

- Requires Rust 1.78 or newer.
- Needs a tokio runtime. The client uses `reqwest` with rustls, so there is no OpenSSL dependency.

## Quick start

The examples on this page use a `tasks` collection with a `title` and a `done` field.

```rust
use k9o::{Client, ClientOpts, KError};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Task {
    id: String,
    title: String,
    done: bool,
}

#[tokio::main]
async fn main() -> Result<(), KError> {
    let client = Client::new(ClientOpts {
        api_key: std::env::var("K9O_API_KEY").ok(),
        org_slug: Some("acme".into()),
        project_slug: Some("todo-app".into()),
        ..Default::default()
    })?;

    let found = client.from("tasks").select().eq("done", false).limit(10).execute::<Task>().await?;
    for task in found.data {
        println!("Output: {} {} done={}", task.id, task.title, task.done);
    }
    Ok(())
}
```

```text
Output: tsk_01H... Write the README done=false
```

Every call returns `Result<Response<T>, KError>`. See [handling errors](#handling-errors).

## Authentication and project context

On the platform, pass `org_slug`, `project_slug` and `api_key`, as in the quick start.
Leave `base_url` out: the client builds the organization host from `org_slug` on the default host.

For a self-hosted or internal gateway, pass `base_url` as its organization host, such as
`https://acme.api.corp.example`. Any `<org>.api.<apex>` host works, and no domain is built in.
Other hosts, such as `localhost`, are used exactly as given:

```rust
let client = Client::new(ClientOpts {
    base_url: Some("https://acme.api.corp.example".into()),
    project_slug: Some("todo-app".into()),
    api_key: std::env::var("K9O_API_KEY").ok(),
    ..Default::default()
})?;
```

- The SDK does not read environment variables. Pass your key in, as shown.
- `project_slug` is required for data, sign-in, storage and other project calls.
- Failed idempotent requests are retried on transient errors. See [client options](#client-options).

End users sign in through Gate. The session tokens come back in `data`, as a `serde_json::Value`:

```rust
use k9o::SignInInput;

let input = SignInInput { email: "user@example.com", password: "correct horse battery staple" };
client.gate().sign_in(input).await?;
println!("Output: signed in");
```

```text
Output: signed in
```

## Running queries

Insert a row. The type parameters are the row you get back and the row you send:

```rust
use serde::Serialize;

#[derive(Serialize)]
struct NewTask {
    title: String,
    done: bool,
}

let new_task = NewTask { title: "Write the README".into(), done: false };
let created = client.from("tasks").insert::<Task, NewTask>(&new_task).await?;
println!("Output: created {}", created.data.id);
```

```text
Output: created tsk_01H...
```

Update and delete both need an `id` filter:

```rust
use serde_json::{json, Value};

let patch = json!({ "done": true });
client.from("tasks").update(patch).eq("id", "tsk_01H...").execute::<Value>().await?;
client.from("tasks").delete().eq("id", "tsk_01H...").execute::<Value>().await?;
```

Read the next page by passing the cursor from `meta` back in:

```rust
let first_page = client.from("tasks").select().limit(10).execute::<Task>().await?;

if let Some(cursor) = first_page.meta.cursor {
    if let Some(next) = cursor.next {
        let second_page = client.from("tasks").select().limit(10).cursor(next).execute::<Task>().await?;
        println!("Output: second page has {} rows", second_page.data.len());
    }
}
```

```text
Output: second page has 10 rows
```

- Filters: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `like`, `ilike`, `is_`, `in_`, `not_in`, `filter`.
- Other services (storage, functions, AI and more) are listed in [functions by namespace](#functions-by-namespace).

## Deploying a function

Upload the compiled artifact first. `upload_artifact` returns the `artifact_key` that `deploy` needs.
The kind is `"tar"` (an OCI image archive of the function's container image, up to 256 MiB) and the tier is `sandbox`. The `gateway` tier is for functions owned by platform operators and is not available to most projects. Sandbox functions run in a [Firelock](https://github.com/firelock-sandbox/firelock) microVM sandbox.

```rust
use k9o::functions::DeployFunctionInput;
use serde_json::json;

let archive = std::fs::read("hello.tar").unwrap();
let artifact = client.functions().upload_artifact(archive, "tar").await?;

client.functions().deploy(DeployFunctionInput {
    name: "hello",
    runtime: "rust",
    tier: "sandbox",
    artifact_key: &artifact.data.artifact_key,
    version: "1.0.0",
    description: None,
}).await?;

let payload = json!({ "name": "Ada" });
let result = client.functions().invoke("hello", Some(&payload), None).await?;
println!("Output: {}", result.data);
```

```text
Output: {"greeting":"Hello, Ada"}
```

`invoke` sends your payload as `{payload}`. Pass `InvokeOptions` as the last argument for `background` and similar controls.

## Handling errors

Failures are returned as `KError`. It implements `std::error::Error`, so `?`, `anyhow` and `eyre` work.
Match on `code`:

```rust
match client.from("tasks").select().execute::<Task>().await {
    Ok(found) => println!("Output: {} tasks", found.data.len()),
    Err(err) => match err.code.as_str() {
        "K9O_TIER_CAP_HIT" => println!("Output: plan limit reached: {}", err.message),
        "K9O_MISSING_PROJECT_CONTEXT" => println!("Output: set project_slug on the client"),
        _ => println!("Output: {}", err),
    },
}
```

Printing a `KError` gives `[code] message`, plus `(status=N)` when the gateway returned it:

```rust
let no_project = Client::new(ClientOpts::default())?;

if let Err(err) = no_project.from("tasks").select().execute::<Task>().await {
    println!("Output: {}", err);
}
```

```text
Output: [K9O_MISSING_PROJECT_CONTEXT] data-plane operations require project_slug to be bound on the client
```

Useful fields are `code`, `message`, `recommendation` and `request_id`.
See [error fields](#error-fields) and [error codes](#error-codes).

## Streams

Create a stream, publish an event and read it back live. `max_bytes` is the storage ceiling in bytes: 1, 2, 4, 8, 16, 32 or 64 GiB.

```rust
use serde_json::json;

let stream = client.streams().create("Orders", 1 << 30, None).await?;
let slug = stream.data.slug;

client.streams().publish(&slug, &json!({ "id": 1, "total": 42 }), None).await?;

let mut events = client.streams().subscribe(&slug, None).await?;
while let Some(event) = events.next().await {
    let event = event?;
    println!("Output: {} {}", event.sequence, event.data);
    events.close();
}
```

```text
Output: 1 {"id":1,"total":42}
```

- `subscribe` returns a handle you read with `next().await`. Call `close` or drop it to stop. Pass a sequence to resume after it.
- `replay` reads a bounded range in one call: `client.streams().replay(&slug, None, None, Some(100))`.
- The server closes each live connection after a short time.
  The subscription reconnects from the last sequence it delivered, so no event is repeated or skipped.
- Publishing needs the project admin role. Subscribing and replaying need admin or auditor.
- While streams are not available, calls fail with `K9O_STREAM_UNAVAILABLE` (HTTP 503).
  A rejected request, such as a bad `subject`, fails with `K9O_STREAM_INVALID`.

## Approvals

Writes sent with `requireApproval` can wait in a queue for a second person. `client.approvals()` reads that queue and decides its changes.
Project admins and auditors see every change; other members see only the ones they proposed.

```rust
use k9o::{ApproveInput, ListApprovalsOptions};

let queue = client
    .approvals()
    .list(ListApprovalsOptions { status: Some("pending_approval"), limit: Some(20) })
    .await?;
let change = &queue.data[0];
println!("Output: {} {} {}", change.id, change.operation, change.collection);

let decided = client
    .approvals()
    .approve(&change.id, ApproveInput { reason: Some("checked the amount") })
    .await?;
println!("Output: {}", decided.data.status);
```

```text
Output: apr_01j9x3 update orders
Output: redeemed
```

- A change is `pending_approval`, `approved`, `rejected`, `expired` or `redeemed` (applied). An `expired` change can still read `pending_approval` until someone decides it.
- `approve` can answer 200 with `status` still `pending_approval`. That means your approval is recorded and more approvers are needed; the change is not applied yet.
- `reject(id, RejectInput { reason })` needs a non-empty `reason`; the SDK checks it before sending.
- `list` is not paginated: it returns the newest `limit` changes (1 to 200, default 50). `get(id)` reads one.
- Deciding needs the queue-admin role (project admin or auditor).

Decide many at once with `bulk`. `criteria` is a predicate over each pending change (see [Approval policies](#approval-policies)):

```rust
use k9o::BulkApprovalsInput;
use serde_json::json;

let criteria = json!({ "eq": ["change.table", "orders"] });
let run = client
    .approvals()
    .bulk(BulkApprovalsInput { action: "reject", criteria: &criteria, reason: Some("stale") })
    .await?;
println!("Output: {} {}", run.data.affected_count, run.data.affected_ids.join(","));
```

```text
Output: 2 apr_01j9x3,apr_01j9x4
```

- A row that fails (a conflict, an apply error) is skipped silently and left out of `affected_ids`. Compare `affected_ids` with the set you intended.
- `reject` needs a `reason`; `criteria` must be a JSON object. `attestation_id` identifies the batch, even when nothing matched.
- It considers the newest 10000 pending changes. Bulk approve is not guaranteed to apply every matched row.

## Approval policies

A policy decides what happens to writes on one collection that match a predicate: `queue` them for approval, or `auto_approve` them.
For a write the first matching policy, oldest first, decides. Policies apply to writes sent with `requireApproval`.

```rust
use k9o::CreateApprovalPolicyInput;
use serde_json::json;

let predicate = json!({ "gt": ["change.row.amount", 1000] });
let policy = client
    .approval_policies()
    .create(CreateApprovalPolicyInput {
        collection: "orders",
        operation: "update",
        action: "queue",
        predicate: &predicate,
    })
    .await?;
println!("Output: {} {}", policy.data.id, policy.data.action);

let removed = client.approval_policies().delete(&policy.data.id).await?;
println!("Output: {}", removed.data.id);
```

```text
Output: apo_01j9x5 queue
Output: apo_01j9x5
```

- `operation` is `insert`, `update` or `delete`; `action` is `auto_approve` or `queue`.
- `collection` matches `^[a-zA-Z_][a-zA-Z0-9_]{0,62}$` and `predicate` must be a JSON object. The SDK checks both before sending.
- A predicate has one operator key: `and`, `or`, `not`, `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in`, `not_in`, `in_cidr`, `not_in_cidr` or `contains`.
  `and` and `or` take a list of predicates, `not` takes one, and a comparison takes `[left, right]`.
- Operands start with `maker.` or `change.` (for example `change.row.amount`, `change.table`, `change.operation`, `maker.id`).
- `list()` returns all policies. Identical policies are not de-duplicated. Creating and deleting need project admin or author.

## Schema collections

`client.schema().collections().list` lists the project's collections by name, with their size and last migration.

```rust
use k9o::ListCollectionsOptions;

let page = client
    .schema()
    .collections()
    .list(ListCollectionsOptions { limit: Some(2), cursor: None })
    .await?;
for c in &page.data {
    println!("Output: {} {} rows, {} columns", c.name, c.row_count, c.column_count);
}
let next = page.meta.cursor.and_then(|c| c.next).unwrap_or_default();
println!("Output: next {next}");
```

```text
Output: orders 120 rows, 8 columns
Output: tasks 14 rows, 4 columns
Output: next dGFza3M
```

- Pass `meta.cursor.next` as `cursor` for the next page; it is `None` on the last page. `limit` is 1 to 200 (default 50).
- A row has `name`, `row_count`, `row_count_is_exact`, `column_count`, `index_count`, `last_migration_slug` and `last_migration_applied_at`.
- `row_count` is a live count that skips soft-deleted rows and ignores row-level security. Without an `org_id` claim on the token, `meta.warnings` says so.
- Project admins, auditors, editors, members and viewers can read it.

## Query with SQL

`query` runs one SQL statement and returns the rows. A positional list binds to `$1`, `$2` and so on:

```rust
use serde_json::{json, Value};

let res = client
    .query::<Value>("SELECT title FROM tasks WHERE done = $1 LIMIT 2", vec![json!(false)])
    .await?;
println!("Output: {} rows, first is {}", res.data.len(), res.data[0]["title"]);
```

```text
Output: 2 rows, first is "Write the README"
```

A `serde_json::Map` gives named values: each `:name` placeholder is rewritten to `$n`, and a name used twice is bound once.
Quoted text, comments, dollar-quoted strings and `::` casts are left alone:

```rust
use serde_json::{json, Map, Value};

let mut params = Map::new();
params.insert("owner".into(), json!("ada"));
params.insert("since".into(), json!("2026-01-01"));
let res = client
    .query::<Value>(
        "SELECT title FROM tasks WHERE data->>'owner' = :owner AND created_at > :since::timestamptz",
        params,
    )
    .await?;
println!("Output: {} rows", res.data.len());
```

```text
Output: 3 rows
```

- Queries are read-only. A write is refused with `K9O_RAW_SQL_READ_ONLY`; change data through `from(collection)`.
- One statement per call, at most 100000 characters of SQL and 1000 parameters. Row-level security applies.
  The caller needs the project-editor role. `dataset.<name>` reads an analytics dataset.
- Values are a string, a number, a boolean or null. Serialise anything else (for example as JSON text) first.
  A placeholder with no key in the map fails with `K9O_INVALID_FIELD_VALUE` before any request.
- Pass `Params::None` (or `()`) for no parameters. `res.meta.raw_query` holds the statement's `intent` and `row_count`.

## Search

`search` finds the best matches for some text in one text column of a collection. `column` and `q` are required;
`language` (default `english`), `search_type` (`Plain`, `Phrase` or `Websearch`), `limit` and `cursor` are optional.

```rust
use k9o::SearchInput;
use serde_json::Value;

let res = client
    .search::<Value>("articles", SearchInput { column: "body", q: "solar panels", limit: Some(2), ..Default::default() })
    .await?;
println!("Output: best match is {}", res.data[0]["title"]);
if res.meta.cursor.and_then(|c| c.next).is_some() {
    println!("Output: more results");
}
```

```text
Output: best match is "Rooftop solar guide"
Output: more results
```

On the select builder, `text_search` and `vector_search` combine with filters, `limit` and `cursor`.
`vector_search` returns the rows nearest to an embedding, closest first.
The metric is `Cosine` (default), `Euclidean` or `Dot`; `limit` is 1 to 1000 (default 10):

```rust
use k9o::{VectorMetric, VectorSearchOptions};
use serde_json::Value;

let res = client
    .from("articles")
    .select()
    .eq("status", "published")
    .vector_search(
        "embedding",
        &[0.12, -0.4, 0.33],
        VectorSearchOptions { metric: VectorMetric::Cosine, limit: Some(3) },
    )
    .execute::<Value>()
    .await?;
println!("Output: {} nearest", res.data.len());
```

```text
Output: 3 nearest
```

An invalid column name, empty query, unknown language or out-of-range limit fails with `K9O_INVALID_FIELD_VALUE`
when the query runs, before any request is sent.

## Generate text

`generate` runs a prompt through an LLM. Only `prompt` is required.

```rust
use k9o::GenerateOptions;

let res = client
    .ai()
    .generate("Say hello in French.", GenerateOptions { max_tokens: Some(20), ..Default::default() })
    .await?;
println!("Output: {}", res.data.content);
println!("Output: {} tokens", res.data.usage.total_tokens);
```

```text
Output: Bonjour !
Output: 14 tokens
```

- `GenerateOptions` has `model`, `max_tokens`, `temperature` and `usage_type`. Unset fields are not sent.
- `model` is optional: the org's default LLM is used. With no default the call fails with HTTP 400.
- Models from an outside provider answer 501 `K9O_NOT_IMPLEMENTED` until they are built.

## Regional hosts

With an organization host (`<org>.api.<apex>`), the client sends one unauthenticated `HEAD` request to find your regional host.
It caches the answer and sends all later requests there. No configuration is needed.
The regional host must share the organization, family and apex of the host you configured.

## Appendix

### Client options

All fields of `ClientOpts` are optional. Use `..Default::default()` for the rest.

| Field | What it does |
|---|---|
| `base_url` | Gateway origin: scheme, host, optional port. No path, query or fragment. Default `https://api.k9o.io` |
| `api_key` | Bearer credential sent with each request |
| `org_slug` | Organization. With a platform host (`api.<apex>`), builds `<org_slug>.api.<apex>`. Also sent as `x-k9o-org-slug` |
| `project_slug` | Project. Sent as `x-k9o-project-slug` and used in project paths |
| `default_headers` | Extra headers added to every request |
| `reqwest_client` | Your own `reqwest::Client` for timeouts, proxies or custom transports |
| `retry_config` | A `RetryConfig`. Default: 3 retries, exponential backoff |

`RetryConfig` fields:

| Field | What it does |
|---|---|
| `max_retries` | Number of retries (default 3) |
| `backoff` | `Backoff::Exponential`, `Backoff::Linear` or `Backoff::None` |
| `initial_delay_ms` | First delay (default 100) |
| `max_delay_ms` | Delay cap (default 5000) |

Retries apply to `GET`, `HEAD`, `OPTIONS` and `DELETE` only. Triggers are transport errors, 429, 502, 503 and 504.

### Functions by namespace

Each namespace is a method on `Client`. Every function is async and returns `Result<Response<T>, KError>`.

#### Data: `client.from("collection")`

| Function | What it does |
|---|---|
| `select()` | Start a query. Chain filters, `limit`, `cursor` and `count`, then `execute::<T>()` |
| `insert(&record)` | Insert one row |
| `update(patch)` | Update one row. Chain `eq("id", ...)`, then `execute` |
| `delete()` | Delete one row. Chain `eq("id", ...)`, then `execute` |
| `select().text_search(column, text, opts)` | Full-text search on a text column |
| `select().vector_search(column, embedding, opts)` | Nearest rows to an embedding, closest first |

#### SQL and search: `client.query`, `client.search`

| Function | What it does |
|---|---|
| `query(sql, params)` | Run one read-only statement; `params` is a `Vec<Value>` (`$n`), a `Map` (`:name`) or `Params::None` |
| `search(collection, SearchInput)` | Full-text search, best matches first |

#### Gate: `client.gate()`

| Function | What it does |
|---|---|
| `sign_up` | Create an end-user identity |
| `sign_in` | Sign in with email and password |
| `sign_out` | End a session. Needs the `refresh_token` |
| `whoami` | Return the current identity |
| `refresh_token` | Exchange a refresh token |

#### Projects: `client.projects()`

| Function | What it does |
|---|---|
| `list` | List projects |
| `create` | Create a project. `display_name` is required; `slug`, `region`, `custody_mode` are optional |
| `get` | Get a project by slug |
| `update` | Update a project, for example its `display_name` |

#### Storage: `client.storage()`

| Function | What it does |
|---|---|
| `list_buckets` | List buckets |
| `get_bucket` | Get one bucket |
| `create_bucket` | Create a bucket |
| `update_bucket` | Change bucket settings |
| `delete_bucket` | Delete a bucket |
| `from(bucket).upload` | Upload an object |
| `from(bucket).create_signed_url` | Create a time-limited URL |
| `from(bucket).list` | List objects, optionally by prefix |
| `from(bucket).remove` | Delete an object |

#### Functions: `client.functions()`

| Function | What it does |
|---|---|
| `upload_artifact` | Upload `"wasm"` or `"tar"` bytes. Returns the `artifact_key` |
| `deploy` | Deploy from an artifact: `name`, `runtime`, `tier`, `artifact_key`, `version` |
| `list` | List functions. `ListFunctionsOptions`: `limit`, `cursor`, `status`, `source`, `tier` |
| `get` | Get one function |
| `delete` | Delete a function |
| `invoke` | Call a function. Sends `{payload}`; optional `InvokeOptions` |

#### AI and compute: `client.ai()`, `client.compute()`

| Function | What it does |
|---|---|
| `ai().embed` | Embed one string or a list of strings |
| `ai().generate` | Run a prompt with the default or a named LLM. Returns `content` and `usage` |
| `compute().llms().list` | List available models |
| `compute().llms().read` | Get one model |
| `compute().llms().invoke` | Run a prompt. The result has `content` and `usage` |
| `compute().llms().invoke_stream` | Run a prompt. Chunks are OpenAI-style: `choices[0].delta.content` |

#### Studio: `client.studio()`

| Function | What it does |
|---|---|
| `workflows().list` | List workflows |
| `workflows().create` | Create a workflow |
| `workflows().read` | Get a workflow |
| `workflows().update` | Update a workflow |
| `workflows().delete` | Delete a workflow |
| `workflows().trigger` | Start a run. Sends `{triggerContext}` |
| `workflows().runs().list` | List runs of a workflow |
| `workflows().runs().read` | Get one run |
| `workflows().runs().cancel` | Cancel a run |
| `dashboards().list` | List dashboards |
| `dashboards().create` | Create a dashboard |
| `dashboards().read` | Get a dashboard |
| `dashboards().update` | Update a dashboard |
| `dashboards().delete` | Delete a dashboard |
| `dashboards().pin` | Pin a dashboard |
| `dashboards().unpin` | Unpin a dashboard |

#### Streams: `client.streams()`

| Function | What it does |
|---|---|
| `create(display_name, max_bytes, retention_seconds)` | Create a stream. `retention_seconds` is an `Option`; `None` uses the default |
| `list` | List streams |
| `get(slug)` | Get one stream |
| `delete(slug)` | Retire a stream |
| `publish(slug, &data, subject)` | Append an event. `subject` is an optional routing suffix |
| `replay(slug, from_sequence, until_sequence, limit)` | Read a range. The result has `events`, `next_cursor`, `has_more` |
| `subscribe(slug, from_sequence)` | Live events via `next().await` (`sequence`, `subject`, `data`, `published_at`). Resumes after a reconnect |

#### Approvals: `client.approvals()`, `client.approval_policies()`, `client.schema()`

| Function | What it does |
|---|---|
| `approvals().list(ListApprovalsOptions)` | Pending changes, newest first: `status`, `limit` (1 to 200). Not paginated |
| `approvals().get(id)` | One pending change |
| `approvals().approve(id, ApproveInput)` | Approve. `status` may stay `pending_approval` while more approvers are needed |
| `approvals().reject(id, RejectInput)` | Reject. `reason` is required |
| `approvals().bulk(BulkApprovalsInput)` | Approve or reject every change matching `criteria`. Compare `affected_ids` with what you intended |
| `approval_policies().list()` | All approval policies, oldest first |
| `approval_policies().create(CreateApprovalPolicyInput)` | Create a policy: `collection`, `operation`, `action`, `predicate` |
| `approval_policies().delete(id)` | Delete a policy |
| `schema().collections().list(ListCollectionsOptions)` | Collections by name: `limit`, `cursor`. Cursor-paginated |

#### Realtime: `client.realtime()`

| Function | What it does |
|---|---|
| `subscribe` | Subscribe to a topic. Read events with `next().await` |

Events have `topic`, `event_type`, `sequence`, `payload` and `occurred_at`.
There is no replay. A `resync` control frame arrives as an event whose `event_type` is empty.
When you see one, re-read current state and subscribe again.

#### Migrations: `client.migrations()`

| Function | What it does |
|---|---|
| `apply` | Apply migrations now |
| `list` | List applied migrations |
| `get` | Get one applied migration by version |
| `queue` | Queue migrations for approval |
| `list_pending` | List queued migrations |
| `apply_pending` | Apply a queued migration |
| `reject_pending` | Reject a queued migration |
| `delete_pending` | Remove a queued migration |

#### Policies: `client.policies()`

| Function | What it does |
|---|---|
| `current` | Get the active policy bundle |
| `list` | List bundles, filtered by `status` only |
| `create` | Create a bundle from a `policy` document (plus optional `rules`, `editor_mode`) |
| `validate` | Check a policy document without saving |
| `get` | Get one bundle |
| `update` | Update a bundle |
| `delete` | Delete a bundle |
| `publish` | Make a bundle active |

#### Audit: `client.audit()`

| Function | What it does |
|---|---|
| `query` | Filters: `event_type`, `actor_id`, `after`, `before`, `limit`, `cursor` |

#### Organization: `client.org()`, `client.orgs()`

| Function | What it does |
|---|---|
| `orgs().list` | List organizations |
| `orgs().get` | Get an organization by slug |
| `orgs().current` | Get the current organization |
| `orgs().restore` | Restore an organization |
| `org().get` | Get the bound organization |
| `org().default_llm_get` | Read the default model. Returns a record, not a string |
| `org().default_llm_set` | Set the default model |
| `org().members()` | `list`, `invite`, `remove` |
| `org().groups()` | `list`, `get` |
| `org().gate().roles()` | Role grants: `list`, `grant`, `get`, `revoke` |
| `org().gate().idp_providers()` | Identity providers: `list`, `create`, `get`, `configure`, `delete` |
| `org().smtp()` | `set` (`password_ref`, `from_address`), `test` (`to`, `subject`), `unset` |
| `org().audit_log().query` | Query the organization audit log (`type`, `limit`) |

#### Current user: `client.me()`

| Function | What it does |
|---|---|
| `profile().get`, `profile().update` | Read or change your profile |
| `sessions()` | `list`, `revoke`, `revoke_all` |
| `api_tokens()` | `list`, `create` (`label`, `scope_manifest`, `ttl_seconds`), `delete` |

#### Vault secrets: `client.kibo().vault().secrets()`

| Function | What it does |
|---|---|
| `list` | List secret metadata, optionally by prefix |
| `get` | Get one secret's metadata by exact name. The region argument is ignored |
| `create` | Seal and store a secret |
| `update` | Store a new value |
| `rotate` | Rotate the wrapping key |
| `move` | Rename a secret path |
| `delete` | Delete a secret |
| `reveal_challenge` | Get a single-use challenge for a reveal |
| `reveal` | Read the plaintext using that challenge |

### Error fields

| Field | Type | Meaning |
|---|---|---|
| `code` | `String` | Stable error code, for example `K9O_TIER_CAP_HIT` |
| `message` | `String` | Human-readable explanation |
| `context` | `HashMap<String, serde_json::Value>` | Structured details for the code |
| `recommendation` | `Option<String>` | Suggested next step |
| `status` | `Option<u16>` | HTTP status, or `None` for errors raised in the SDK |
| `request_id` | `Option<String>` | Gateway trace id |

### Error codes

The gateway can return other codes. They are passed through unchanged.

| Code | Raised by | When |
|---|---|---|
| `K9O_INVALID_BASE_URL` | SDK | `base_url` is not a valid origin |
| `K9O_INVALID_FIELD_VALUE` | SDK | Bad slug, collection name or input, or a missing `id` filter |
| `K9O_MISSING_PROJECT_CONTEXT` | SDK | A project call was made without `project_slug` |
| `K9O_UPSTREAM_UNAVAILABLE` | SDK | The connection or stream to the gateway failed |
| `K9O_INTERNAL_ERROR` | SDK | The response could not be read |
| `K9O_TIER_CAP_HIT` | Gateway | A plan limit was reached |
| `K9O_PROJECT_NOT_FOUND` | Gateway | The project does not exist |
| `K9O_STREAM_UNAVAILABLE` | Gateway | Streams are not available yet; retry later |
| `K9O_STREAM_INVALID` | Gateway | A stream request was rejected, for example a bad `subject` |

### Related packages

There are no companion crates published alongside `k9o`.

### About Kilimanjaro

Kilimanjaro is a modular developer platform for building secure enterprise applications.
You define AI agents, functions and data pipelines on the platform.
The SDKs let you deploy and invoke them, read and write your data, publish to and subscribe to streams, sign users in,
and encrypt with keys you hold.

It provides four building blocks:

- **Keep**: AI security and governance. Guardrails and policy for the agents you run.
- **Pulse**: data intelligence and real-time stream processing. Streams, analytics and live feeds.
- **Gate**: customer identity and access management. Sign-in and sessions, and automatic data scoping
  so a signed-in user reaches only their own data.
- **Kibo**: cryptographic keys and secrets management. Client-side encryption with keys you hold, and a secrets vault.

## License

Apache-2.0 © Formelo Inc. See `LICENSE` and `NOTICE`.
