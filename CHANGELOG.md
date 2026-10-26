# Changelog — k9o (Rust crate)

## 1.4.0 (unreleased)

### Added

- `query`: run one read-only SQL statement with positional or `:name` parameters.
- `search`: full-text search over one column of a collection.
- `text_search` and `vector_search` on the select builder, for full-text and nearest-neighbour search.
- `ai().generate`: run a prompt through an LLM, with an optional `model`. Same result as `compute().llms().invoke`.
- `streams()`: `create`, `list`, `get`, `delete`, `publish`, `replay` and `subscribe` for user data streams. `subscribe` reconnects from the last sequence.
- `approvals()`: `list`, `get`, `approve`, `reject` and `bulk` for the pending-change queue.
- `approval_policies()`: `list`, `create` and `delete` for the rules that decide which writes are queued.
- `schema().collections().list`: the project's collections by name, with cursor paging.

### Changed

- README description.
- Function deploy examples use the sandbox tier.

## 1.3.0 — 2026-10-26

Every method now matches the gateway and is checked by a contract test.

### Changed

- `projects().create` takes `display_name` (required) and optional `slug`, `region`, `custody_mode`. `get` and `update` use the projects routes.
- `functions().deploy` takes `name`, `runtime`, `tier` (`gateway` or `sandbox`), `artifact_key`, `version` and `description`. Inline code is gone.
- `functions().invoke` sends `{payload}` and accepts `InvokeOptions`. `functions().list` accepts `limit`, `cursor`, `status`, `source`, `tier`.
- Policies `create`, `update` and `validate` take a `policy` document. `policies().list` takes only `status`.
- `audit().query` filters are `event_type`, `actor_id`, `after`, `before`, `limit`, `cursor`.
- Role grants use `/role-grants` and identity providers use `/identity-providers`, with typed create and update inputs.
- `default_llm_get` and `default_llm_set` return records. `smtp().set` takes `password_ref` and `from_address`; `test` takes `to` and `subject`.
- `api_tokens().create` takes `label`, `scope_manifest` and `ttl_seconds`. `sessions().revoke` and `revoke_all` use the session routes.
- `compute().llms().invoke` parameters are flat and the result has `content` and `usage`. Stream chunks are OpenAI-style.
- `workflows().trigger` sends `{triggerContext}`. `gate().sign_out` requires the refresh token.
- `kibo().vault().secrets().get` looks up a secret by exact name; the region is optional.
- The API host is read from the base URL: any `<org>.api.<apex>` host works and no domain is built in.

### Removed

- `gate().verify_session`, `gate().hosted`, `realtime().replay` and the polling transport.
- `me().notifications`, `me().audit_log`, `me().data_export`.
- `org().groups().create`, `org().groups().delete`, `org().members().get`, `org().smtp().get`.
- The `modality` filter of `compute().llms().list` and the `triggerType` filter of `workflows().list`.

### Added

- `functions().upload_artifact(bytes, "wasm" | "tar")` returns the `artifact_key` for `deploy`.

## 1.2.0 — 2026-10-25

- The README now introduces the Kilimanjaro building blocks (Keep, Pulse, Gate, Kibo). No change to the public API.

## 1.1.1 — 2026-10-25

- Documentation links corrected: the documentation hub is https://docs.kilimanjaro.dev, with the website, platform API and console listed in a Links section. Package metadata points at the same places.

## 1.1.0 — 2026-10-25

- Documentation rewritten with runnable examples and expected output; internal cross-references removed from the published sources. No change to the public API.
- Error text for `update()` / `delete()` without an `id` filter no longer mentions a version or a roadmap.

## 1.0.0 — 2026-10-23

First public release. See the README for the full API overview.
