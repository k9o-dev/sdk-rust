# k9o

Rust SDK for the Kilimanjaro (k9o) platform: an async client for data, sign-in, storage, functions, AI and more.

Kilimanjaro is an AI-native platform on a Governed Database. Its products are Keep (AI Security and Governance), Pulse (Data Intelligence), Gate (Customer Identity and Access Management) and Kibo (Keys and Secrets Management).

## Contents

- [Links](#links)
- [Install](#install)
- [Quick start](#quick-start)
- [Authentication and project context](#authentication-and-project-context)
- [Running queries](#running-queries)
- [Handling errors](#handling-errors)
- [Regional hosts](#regional-hosts)
- [Appendix](#appendix): [client options](#client-options), [functions](#functions-by-namespace),

## Links

- Website: <https://kilimanjaro.dev>
- Documentation / Developer Hub: <https://docs.kilimanjaro.dev>
- Platform API: <https://api.k9o.io>
- Platform Console: <https://console.k9o.io>
  [error fields](#error-fields), [error codes](#error-codes), [related packages](#related-packages)

## Install

```toml
[dependencies]
k9o = "1.0"
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
        println!("{} {} done={}", task.id, task.title, task.done);
    }
    Ok(())
}
```

```text
tsk_01H... Write the README done=false
```

Every call returns `Result<Response<T>, KError>`. See [handling errors](#handling-errors).

## Authentication and project context

On the platform, pass `org_slug`, `project_slug` and `api_key`, as in the quick start.
Leave `base_url` out: the client derives `https://<org_slug>.api.k9o.io` for you.

For a custom or self-hosted host, pass `base_url` instead of `org_slug`:

```rust
let client = Client::new(ClientOpts {
    base_url: Some("https://k9o.internal.example.com".into()),
    project_slug: Some("todo-app".into()),
    api_key: std::env::var("K9O_API_KEY").ok(),
    ..Default::default()
})?;
```

- The SDK does not read environment variables. Pass your key in, as shown.
- `project_slug` is required for data, sign-in, storage and other project calls.
- Failed idempotent requests are retried on transient errors. See [client options](#client-options).

End users sign in through Gate. The session comes back as a `serde_json::Value`:

```rust
use k9o::SignInInput;

let input = SignInInput { email: "user@example.com", password: "correct horse battery staple" };
let session = client.gate().sign_in(input).await?;
println!("{}", session.data);
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
println!("created {}", created.data.id);
```

```text
created tsk_01H...
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
        println!("second page has {} rows", second_page.data.len());
    }
}
```

```text
second page has 10 rows
```

- Filters: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `like`, `ilike`, `is_`, `in_`, `not_in`, `filter`.
- Other services (storage, functions, AI and more) are listed in [functions by namespace](#functions-by-namespace).

## Handling errors

Failures are returned as `KError`. It implements `std::error::Error`, so `?`, `anyhow` and `eyre` work.
Match on `code`:

```rust
match client.from("tasks").select().execute::<Task>().await {
    Ok(found) => println!("{} tasks", found.data.len()),
    Err(err) => match err.code.as_str() {
        "K9O_TIER_CAP_HIT" => println!("plan limit reached: {}", err.message),
        "K9O_MISSING_PROJECT_CONTEXT" => println!("set project_slug on the client"),
        _ => println!("{}", err),
    },
}
```

Printing a `KError` gives `[code] message`, plus `(status=N)` when the gateway returned it:

```rust
let no_project = Client::new(ClientOpts::default())?;

if let Err(err) = no_project.from("tasks").select().execute::<Task>().await {
    println!("{}", err);
}
```

```text
[K9O_MISSING_PROJECT_CONTEXT] data-plane operations require project_slug to be bound on the client
```

Useful fields are `code`, `message`, `recommendation` and `request_id`.
See [error fields](#error-fields) and [error codes](#error-codes).

## Regional hosts

With `org_slug` and the default host, the client sends one unauthenticated `HEAD` request to find your regional host.
It caches the answer and sends all later requests there. No configuration is needed.

## Appendix

### Client options

All fields of `ClientOpts` are optional. Use `..Default::default()` for the rest.

| Field | What it does |
|---|---|
| `base_url` | Gateway origin: scheme, host, optional port. No path, query or fragment. Default `https://api.k9o.io` |
| `api_key` | Bearer credential sent with each request |
| `org_slug` | Organization. With the default host, selects `https://<org_slug>.api.k9o.io`. Also sent as `x-k9o-org-slug` |
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

#### Gate: `client.gate()`

| Function | What it does |
|---|---|
| `sign_up` | Create an end-user identity |
| `sign_in` | Sign in with email and password |
| `sign_out` | End a session |
| `whoami` | Return the current identity |
| `refresh_token` | Exchange a refresh token |
| `verify_session` | Check the current session |
| `hosted` | Hosted sign-in URL and relying-party id |

#### Projects: `client.projects()`

| Function | What it does |
|---|---|
| `list` | List projects |
| `create` | Create a project |
| `get` | Get a project by slug |
| `update` | Update a project |

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
| `deploy` | Deploy a function |
| `list` | List functions |
| `get` | Get one function |
| `delete` | Delete a function |
| `invoke` | Call a function with a JSON payload |

#### AI and compute: `client.ai()`, `client.compute()`

| Function | What it does |
|---|---|
| `ai().embed` | Embed one string or a list of strings |
| `compute().llms().list` | List available models |
| `compute().llms().read` | Get one model |
| `compute().llms().invoke` | Run a prompt and get the completion |
| `compute().llms().invoke_stream` | Run a prompt and read tokens as a stream |

#### Studio: `client.studio()`

| Function | What it does |
|---|---|
| `workflows().list` | List workflows |
| `workflows().create` | Create a workflow |
| `workflows().read` | Get a workflow |
| `workflows().update` | Update a workflow |
| `workflows().delete` | Delete a workflow |
| `workflows().trigger` | Start a run |
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

#### Realtime: `client.realtime()`

| Function | What it does |
|---|---|
| `subscribe` | Subscribe to a topic. Read events with `next().await` |
| `replay` | Fetch past events from a sequence number |

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
| `list` | List bundles |
| `create` | Create a bundle |
| `validate` | Check rules without saving |
| `get` | Get one bundle |
| `update` | Update a bundle |
| `delete` | Delete a bundle |
| `publish` | Make a bundle active |

#### Audit: `client.audit()`

| Function | What it does |
|---|---|
| `query` | Query the project audit log, newest first, with `AuditQueryOpts` filters |

#### Organization: `client.org()`, `client.orgs()`

| Function | What it does |
|---|---|
| `orgs().list` | List organizations |
| `orgs().get` | Get an organization by slug |
| `orgs().current` | Get the current organization |
| `orgs().restore` | Restore an organization |
| `org().get` | Get the bound organization |
| `org().default_llm_get` | Read the default model |
| `org().default_llm_set` | Set the default model |
| `org().members()` | `list`, `invite`, `get`, `remove` |
| `org().groups()` | `list`, `create`, `get`, `delete` |
| `org().gate().roles()` | `list`, `grant`, `get`, `revoke` |
| `org().gate().idp_providers()` | `list`, `create`, `get`, `configure`, `delete` |
| `org().smtp()` | `get`, `set`, `test`, `unset` |
| `org().audit_log().query` | Query the organization audit log |

#### Current user: `client.me()`

| Function | What it does |
|---|---|
| `profile().get`, `profile().update` | Read or change your profile |
| `sessions()` | `list`, `revoke`, `revoke_all` |
| `api_tokens()` | `list`, `create`, `delete` |
| `notifications()` | `list`, `mark_read`, `mark_all_read` |
| `audit_log().query` | Query your audit log |
| `data_export()` | `request_export`, `status` |

#### Vault secrets: `client.kibo().vault().secrets()`

| Function | What it does |
|---|---|
| `list` | List secret metadata, optionally by prefix |
| `get` | Get one secret's metadata |
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
| `K9O_INVALID_FILTER` | SDK | `replay` was given a negative sequence |
| `K9O_UPSTREAM_UNAVAILABLE` | SDK | The connection or stream to the gateway failed |
| `K9O_INTERNAL_ERROR` | SDK | The response could not be read |
| `K9O_TIER_CAP_HIT` | Gateway | A plan limit was reached |
| `K9O_PROJECT_NOT_FOUND` | Gateway | The project does not exist |

### Related packages

There are no companion crates published alongside `k9o`.

## License

Apache-2.0 © Formelo Inc. See `LICENSE` and `NOTICE`.
