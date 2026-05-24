# Audit: agentflow-tracing

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-tracing/
**Crate version**: 0.1.0 (workspace targets v0.3.0)
**Layer**: L4 (Operations / Productization)
**Stability tier**: alpha (file backend GA; postgres feature-gated and not wired; OTel exporter shape stable but no first-party OTLP transport)

## Scope summary

`agentflow-tracing` consumes `agentflow_core::events::WorkflowEvent` via the
`EventListener` trait and assembles a hierarchical `ExecutionTrace`
(workflow → node → agent → tool / LLM) for persistence, redaction, replay,
TUI rendering, and OTel span export. The crate is intentionally non-invasive:
it only depends on `agentflow-core`, `serde`, `tokio`, `chrono`, `anyhow`,
`thiserror`, `async-trait`, and (optionally) `sqlx`.

Implementation surface audited:
- `collector.rs` — `TraceCollector`, drain task, terminal trace export
- `redaction.rs` — JSON tree + plain-text redaction
- `otel.rs` — span model, `OtelSpanSink`, `TraceExporter`, `trace_to_spans`
- `storage/{mod,file,schema}.rs` — `TraceStorage` trait, file backend, DDL
- `context.rs` — W3C `traceparent` task-local
- `replay.rs`, `tui.rs`, `format.rs` — read-only renderers
- `types.rs` — `ExecutionTrace`, `NodeTrace`, `AgentTrace`, `ToolCallTrace`
- `benches/event_write.rs`, `tests/hybrid_trace_replay_fixture.rs`,
  `examples/simple_tracing.rs`

## Findings

### CRITICAL

- **[C1] `StorageErrorPolicy::FailWorkflow` panics inside the drain task, taking down the listener loop and silently dropping every subsequent event** — `src/collector.rs:419-431` (`handle_storage_error`) called from `src/collector.rs:481` (drain loop) and `src/collector.rs:504` (sync mode).
  **What**: When `on_storage_error == FailWorkflow`, `handle_storage_error` calls `panic!()`. In the unbounded-channel drain task (`tokio::spawn` at line 469), that panic kills the consumer task; the `UnboundedSender` survives via `OnceLock` so subsequent `on_event` calls silently `tx.send(event)` into a dead channel (the `let _ = ...` at line 489 swallows the error). All future workflow traces are lost without any signal to the caller.
  **Why it matters**: The variant is documented as "not recommended for production", but it does not actually fail the workflow — it disables tracing for the entire process while the workflow keeps running blind. This is exactly the silent-data-loss failure mode the drain task was added to prevent.
  **Fix**: Either (a) drop `FailWorkflow` from the public surface and return a `Result` from `handle_storage_error` so the drain task can short-circuit cleanly, (b) make the policy fail the workflow at the `EventListener` boundary by panicking in `on_event` directly (sync path, before the channel hop), or (c) restart the drain task on panic and emit a `tracing::error!` so the operator can wire an alert.

### MAJOR

- **[M1] Drain task uses an unbounded channel with no backpressure or drop counter** — `src/collector.rs:111, 463, 489`.
  **What**: `tokio::sync::mpsc::unbounded_channel::<WorkflowEvent>` is allocated on first event. If storage is slow (disk-full, postgres degraded, OTel sink stuck), the queue grows without limit; with `let _ = tx.send(event)` there is no signaling, metric, or `tracing::warn!`. Memory keeps climbing until OOM.
  **Why it matters**: This is the "disk full / backpressure" path the audit brief calls out. Today it neither blocks nor drops — it leaks. There is no observability for trace ingest lag.
  **Fix**: Switch to a bounded channel sized via `TraceConfig` (e.g. 8K events default). On `try_send` failure, emit `tracing::warn!` once per high-watermark crossing plus a monotonic dropped-event counter exposed on `TraceCollector`. Document the policy ("drop newest" vs. "block sender") explicitly.

- **[M2] `process_event` calls `Self::handle_storage_error` for every exporter failure, which means a single misconfigured OTel sink can panic the drain task** — `src/collector.rs:401-411`.
  **What**: `export_trace_to_sinks` iterates exporters and routes `Err` through `handle_storage_error`. With `FailWorkflow` policy a remote OTLP outage panics the listener task (see C1); even with `LogError` policy the loop continues to the next exporter, but a slow async sink (`export_trace.await`) blocks the drain task because export happens inline during event processing under the active workflow's `write` lock would not be held here — yet the drain task itself stalls, so every queued event (LLM responses, node completions for other workflows) backs up behind the slow exporter.
  **Why it matters**: Production tracing must isolate sink latency from in-process event handling. As written, one stuck OTLP endpoint slows or breaks tracing for every workflow.
  **Fix**: Spawn exporter calls on a separate task (one per exporter or one shared) with their own bounded queue, or add a per-exporter timeout (`tokio::time::timeout` around `export_trace`). Make sink configuration include a "best effort vs. blocking" knob.

- **[M3] No OTLP HTTP/gRPC transport, no TLS surface, no auth header — `OtelSpanSink` is a trait that nothing in the workspace implements outside of tests** — Confirmed via `grep`: only `RecordingSink` in `otel.rs:564` and no consumer in CLI/server.
  **What**: The roadmap (CLAUDE.md, L4 — agentflow-tracing) advertises an "OTel OTLP exporter with W3C trace context propagation" as production-ready, but the crate only ships the in-memory boundary trait. Cargo.toml has no `opentelemetry`, `opentelemetry-otlp`, `opentelemetry-sdk`, or `reqwest` dependency. `TraceCollector` is constructed without `.with_exporter(...)` in `agentflow-cli/src/commands/workflow/run.rs:89` and `agentflow-server/src/runs.rs:387`.
  **Why it matters**: The stability surface ("OTel exporter") is documented but absent. Operators who follow the doc will find no path from `TraceCollector` to an OTLP collector without writing their own HTTP/gRPC sink, TLS config, retry, and auth headers from scratch.
  **Fix**: Either (a) implement a minimal `OtlpHttpSink` (feature-flagged `otlp-http`, depends on `reqwest = { ... no-default-features, rustls-tls }`) with `OTEL_EXPORTER_OTLP_ENDPOINT` / `OTEL_EXPORTER_OTLP_HEADERS` env support, or (b) update CLAUDE.md to mark OTel as "schema only, BYO transport" and add a worked example referencing the official `opentelemetry-otlp` crate. The current state misrepresents maturity.

- **[M4] OTel `trace_id` and `span_id` use a homemade FNV-1a hash, not W3C-compliant 128-bit random IDs** — `src/otel.rs:527-551` (`trace_id`, `span_id`, `hex_hash`).
  **What**: IDs are derived from `hex_hash(workflow_id, 16)` for traces and `hex_hash("{workflow_id}:{name}", 8)` for spans. The hash extends itself with `hash ^= hash.rotate_left(13)` to fill the required width. W3C trace context requires that trace IDs be random 16-byte values (RFC 9110 §4.2.2 specifies that all-zero IDs are invalid; `00000000000000000000000000000000` is the explicit invalid value). FNV is deterministic and trivially collidable; two workflows with the same `workflow_id` produce the same `trace_id`, breaking the assumption that the OTel backend uses for span correlation across reruns.
  **Why it matters**: The IDs are stable across reruns (deterministic), which means OTel backends will merge unrelated runs sharing a `workflow_id` and any tooling that expects random IDs (e.g. Tempo, Jaeger sampling decisions) will misbehave. Stitching with W3C `traceparent` (propagated in `context.rs`) is broken because incoming `traceparent` is never used to set this trace ID.
  **Fix**: Generate random IDs (e.g. `rand::random::<[u8; 16]>()`) at trace creation, store them on `ExecutionTrace`/`TraceContext`, and consume the active `traceparent` from `context::current_traceparent()` to override the trace ID when an upstream context exists. Persist the IDs alongside the trace so reruns produce distinct trace IDs.

- **[M5] `traceparent` propagation in `context.rs` is never consumed by the trace collector or OTel span emitter** — Confirmed: `current_traceparent()` is only referenced from `agentflow-llm` and `agentflow-plugins` (not in this crate); `trace_to_spans` builds IDs from `workflow_id` only.
  **What**: The crate ships an installer (`scope`) and reader (`current_traceparent`), but the trace pipeline does not honor an upstream `traceparent`. So when an external service hands AgentFlow a W3C context, `OtelSpan.trace_id` is computed from `workflow_id` and the upstream trace is orphaned.
  **Why it matters**: CLAUDE.md claims "W3C `traceparent` propagation through LLM HTTP calls" — true for outbound LLM calls, but inbound context is dropped. Cross-service stitching does not actually work end-to-end.
  **Fix**: At `WorkflowStarted` handling (`collector.rs:174-183`) consult `crate::context::current_traceparent()`; if present, parse and use the trace ID as the canonical `trace_id`. Add an integration test asserting that a workflow run inside `context::scope(...)` emits spans with the matching trace ID.

- **[M6] `FileTraceStorage` has no fsync, no cleanup sweep wired by any consumer, and no on-disk permissions hardening** — `src/storage/file.rs:78-83`, `136-158`; `delete_old_traces` is implemented but `grep` finds no production caller.
  **What**: `save_trace` writes via `tokio::fs::write` (no fsync) so a crash between write and flush can leave a zero-byte or partial JSON file. `delete_old_traces` exists but is never invoked from CLI, server, or worker. Files are created with default umask permissions (typically `0644`) so any local user can read trace JSON containing redacted (but still attacker-interesting) data.
  **Why it matters**: Brief explicitly flags "File trace directories: permissions, cleanup sweep?". Today: no cleanup, no permission tightening, no fsync. The CLI default writes to `~/.agentflow/traces` which on a shared workstation is world-readable.
  **Fix**: (a) Add `fs::OpenOptions` with `mode(0o600)` on unix targets for trace files; (b) wire `delete_old_traces` to an opt-in retention policy or the existing `agentflow cleanup` command (`agentflow-cli/src/commands/cleanup.rs`); (c) add explicit `flush + sync_data` semantics or switch to write-rename for crash safety.

- **[M7] `FileTraceStorage::query_traces` reads and deserializes every JSON file in the directory on every query** — `src/storage/file.rs:97-134`.
  **What**: For each query, the implementation calls `fs::read_dir`, then for every `.json` entry reads the whole file and parses it with serde. With 10K traces this is O(N) disk reads per query.
  **Why it matters**: Brief calls this out under "Performance + Dependencies". Even moderate trace volumes will make `agentflow trace tui` and any server-side list operation unusable.
  **Fix**: Sort filesystem entries by mtime first (FS already maintains this), short-circuit on `limit`. Or — better — switch to a per-day index file (`index-YYYY-MM-DD.jsonl`) that records `{workflow_id, started_at, status, tags}` so list operations don't need to parse every full trace. For production, recommend the postgres backend.

- **[M8] Redaction key matcher is a substring contains check, not a word-boundary match — `safe_token` and `etoken` both match the `"token"` pattern** — `src/redaction.rs:223-233`.
  **What**: `is_sensitive_key` does `normalized.contains(&pattern)`. With pattern `"token"`, the normalized key `usertokencount` is redacted, but conversely `"secretly_known"` also matches `"secret"`. The current default patterns also fail to match common variants like `"x-secret"` (matches `secret` — OK), `"openai_organization_id"` (does not match), `"hf_token"` (matches `token` — OK), `"jwt"` (NOT matched), `"refresh_token"` (matches — OK).
  **Why it matters**: Brief: "Redaction completeness ... gaps are MAJOR." JWTs, refresh tokens (covered), session cookies (`cookie` not in default list), `set-cookie` (not covered), AWS credentials (`aws_access_key_id`, `aws_secret_access_key` — the second matches via `secret`, but the first does not), Slack webhooks (`webhook_url` not covered), private keys (`private_key` covered but `ssh_key` not).
  **Fix**: Add `jwt`, `cookie`, `set_cookie`, `webhook`, `signature`, `client_secret`, `refresh_token` (explicit), `aws_access_key_id`, `aws_secret`, `ssh_key`, `pgp` to `default_sensitive_key_patterns`. Consider a strict-mode matcher that requires the pattern to be a separator-delimited word.

- **[M9] `redact_text` only redacts after whitespace tokenization — multi-line headers, JSON-in-string, and URL query strings leak** — `src/redaction.rs:146-196`.
  **What**: `redact_bearer_tokens` matches `Bearer <next-whitespace-token>` — so `Bearer\nABCDEF` (CRLF header continuation) is missed because `\n` is whitespace and the next token starts on a new "line". URL query strings like `https://api.example/data?api_key=secret&q=test` are a single whitespace-delimited token; `redact_assignment_token` splits only on the *first* `=`/`:`, so the `api_key=secret&q=test` chunk replaces the entire suffix with `[REDACTED]&q=test` only if it finds `api_key` before any other `=`. Actually it splits on the first `=` so the redacted output is `https://api.example/data?api_key=[REDACTED]` — but `q=test` is gone. Conversely `body={"api_key":"secret"}` is one token; the `:` split puts the redaction over the value, but everything after the first `:` is replaced.
  **Why it matters**: Inline-token redaction silently destroys legitimate trailing structure (query string filters, JSON body fragments) while still leaking when secrets appear with non-whitespace separators (`&`, `;`, `\n` continuations).
  **Fix**: For text redaction add URL query parsing (`url` crate is already in workspace via reqwest deps elsewhere) plus a tolerant JSON-substring detector. Add tests for `?api_key=...&q=...`, `Bearer \r\n abc`, `Authorization: Basic <base64>` (Basic is not handled today).

- **[M10] `limit_value_size` runs `redact_value` with a hard-coded `RedactionConfig::default()` instead of the collector's config** — `src/collector.rs:435-443`.
  **What**: Inside `NodeOutputCaptured` handling the code calls `Self::limit_value_size(&mut output, ...)` which mutates `output` to enforce a size cap and redacts with `RedactionConfig::default()`. Operators that set `RedactionConfig::disabled()` (e.g. for replay debugging) still have their captured outputs redacted because `limit_value_size` ignores `config.redaction`.
  **Why it matters**: The redaction setting silently fails to round-trip. Brief: documentation/security boundary mismatch.
  **Fix**: Pass `&config.redaction` into `limit_value_size`, or split the size check from redaction entirely (size truncation belongs upstream of redaction, not entangled with it).

- **[M11] `process_event` swallows the `last node row update` race for any non-rev match** — `src/collector.rs:209-215, 254-260, 269-275, 344-349`.
  **What**: All node lookups use `trace.nodes.iter_mut().rev().find(|n| n.node_id == node_id)`. If the same node id appears twice (e.g. retried Map sub-node, While loop body), the `rev()` picks the latest — but `NodeStarted` always pushes a *new* row. So a retried node creates a fresh entry rather than reopening the previous one, and the previous row is left at `status=Running` forever in the persisted trace.
  **Why it matters**: Replay/TUI shows a phantom "running" node that never completes, which is the exact symptom the drain-task arrival-order fix was meant to eliminate. The race fix prevented misordering between concurrent workflows; this leak is a separate semantic bug for any retry/loop scenario.
  **Fix**: Either (a) match on `(node_id, status == Running)` so node lookups only target the open row, or (b) embed a per-step `attempt` counter in `WorkflowEvent::NodeStarted` and link it through completion events.

### MINOR

- **[m1] `TraceConfig::development()` sets `on_storage_error: StorageErrorPolicy::Ignore` — dev-mode tracing errors are completely invisible** — `src/collector.rs:67`. Recommend `LogError` for both presets; reserve `Ignore` for explicit opt-in.

- **[m2] `block_on(async {...})` inside `on_event` when `async_storage == false` will panic if called from a sync context with no current runtime, and will deadlock if called from inside a `tokio::runtime::Handle::current()` worker** — `src/collector.rs:490-507`. The sync path is documented as "for testing or special cases" but the failure mode is not obvious. Either remove the sync mode or guard with `Handle::try_current()` and return early with `eprintln!`.

- **[m3] `unwrap()` on `completed_at` in `NodeTrace::complete`/`fail`** — `src/types.rs:204, 216`. Self-set immediately above, so safe in practice, but violates CLAUDE.md "Never use `unwrap()`" rule. Replace with `self.completed_at.unwrap_or(self.started_at)` or hoist to a local variable.

- **[m4] `timestamp_nanos_opt().unwrap_or_default()` silently emits `0` for `DateTime`s outside the i64 nanosecond range** — `src/otel.rs:519-521`. For traces of running workflows where `completed_at` is `None`, the workflow end-time falls back to `start`, masking that the trace is open. Add an attribute (`agentflow.workflow.completed: false`) when emitting spans for in-flight traces.

- **[m5] Postgres schema is published as `&'static str` constants but no `sqlx::PgPool`-based `TraceStorage` impl exists in the crate** — `src/storage/schema.rs:9-102`; cargo `postgres` feature pulls `sqlx` but has zero implementation. Either remove the feature flag from `Cargo.toml` or add a stub `PostgresTraceStorage` that runs the schema and implements `TraceStorage`. As shipped, enabling `--features postgres` compiles sqlx with no consumer.

- **[m6] No `tracing` crate dependency — error reporting uses `eprintln!`** — `src/collector.rs:425`. Operators running under systemd or under structured-log capture cannot route trace-storage warnings/errors. Add `tracing` and emit `tracing::warn!`/`tracing::error!`.

- **[m7] `redact_text` does not handle `Basic <base64>` (HTTP basic auth)** — `src/redaction.rs:146-165`. Only `bearer` is special-cased. Add `basic`, `negotiate`, `digest` token prefixes from RFC 7235.

- **[m8] `is_environment_variable_name_key` hardcodes 5 specific keys but the input `api_key_env` only matches by string equality after normalization** — `src/redaction.rs:243-248`. If a user names their field `openai_api_key_envname` it gets redacted because contains-match wins. Document the exact allowlist and add a test for the boundary case.

- **[m9] `FileTraceStorage::matches_query` skips files with deserialization errors silently** — `src/storage/file.rs:110-117` (`if let Ok(json) = ... && let Ok(trace) = ...`). Corrupt or schema-mismatched files vanish from results with no operator signal. Log a warning and surface a count.

- **[m10] `OtelTraceExporter` ignores its `OtelExporterConfig.environment` field if `trace.metadata.environment` is set on the trace** — `src/otel.rs:266-275`. The `or(Some(&trace.metadata.environment))` form makes per-trace metadata override exporter-level config, which is the opposite of typical OTel semantics where deployment-level config wins. Either invert or document.

- **[m11] No public rustdoc on `TraceContext::workflow`/`child`, `OtelSpan` fields, or `OtelValue` variants** — `src/types.rs:94-112`, `src/otel.rs:94-153`. The shapes are stable enough to expose; missing docs will fail `cargo doc -D missing-docs` if that lint is ever enabled.

- **[m12] `panic!("Trace storage failed: ...")` does not include `workflow_id` or trace context** — `src/collector.rs:428`. The post-mortem stack trace from a panic in the spawned drain task hides which workflow caused the failure.

### POSITIVE OBSERVATIONS

- The drain-task design (`std::sync::OnceLock` + single dedicated consumer) correctly fixes the `WorkflowCompleted` race the CLAUDE.md notes call out — the field-level comment on `drain_tx` is excellent documentation of *why* the design exists.
- Redaction is applied uniformly via `prepare_terminal_trace` before both storage *and* exporter calls (`src/collector.rs:363-369, 384-390`), with a covering test `test_trace_collector_redacts_terminal_trace_before_storage_and_export` (line 641). This is the right boundary.
- `RedactionConfig` correctly distinguishes "value is sensitive" from "value is the *name* of a sensitive env var" via `is_environment_variable_name_key`, with `keeps_environment_variable_names_visible` regression test.
- `AgentTrace::collect_tool_calls` ties LLM tool calls, MCP routing, policy decisions, and idempotency / side-effect classification into a structured shape with span-context linkage (`attach_context`) — sophisticated and well-tested (`test_agent_trace_context_links_tool_calls`).
- `format_trace_replay`, `format_trace_tui`, `format_trace_human_readable`, and `export_trace_json` all eagerly redact a clone before rendering, so no rendering path can leak a not-yet-redacted trace.
- Cross-hop W3C `traceparent` is correctly modeled as a `tokio::task_local!` with a documented "no upstream → emit nothing" contract; the env-var spelling is pinned with an assertion test.
- Schema constants for Postgres + SQLite share index coverage and `ON DELETE CASCADE` semantics, with a structural test that prevents accidental drift.
- Benches exist (`event_write.rs`) covering serialize-only, file-storage round trip, and synthetic JSONL append — the right three points of comparison for a future SQLite backend.
- The hybrid trace fixture (`tests/fixtures/hybrid_trace_replay.json` + `hybrid_trace_replay_fixture.rs`) is a good golden-file test that locks the replay output shape across template / skill_agent / MCP layers.

## Metrics

- Source files: 12 (`collector.rs`, `context.rs`, `format.rs`, `lib.rs`,
  `otel.rs`, `redaction.rs`, `replay.rs`, `tui.rs`, `types.rs`,
  `storage/mod.rs`, `storage/file.rs`, `storage/schema.rs`)
- Lines of code: 4,430 (including in-file tests)
- Persistence backends: 1 implemented (`FileTraceStorage`) + 1 schema-only
  (Postgres DDL constant, no `TraceStorage` impl), in-memory `current_traces`
  for running workflows
- Test files: 11 modules with `#[cfg(test)]` (in-file) + 1 integration test
  + 1 benchmark + 1 example
- `unwrap()/expect()` in non-test code: 2
  - `src/types.rs:204` — `NodeTrace::complete` unwraps `completed_at` it just set
  - `src/types.rs:216` — `NodeTrace::fail` same pattern
  Both are guarded by `self.completed_at = Some(Utc::now())` immediately above.
- TODO/FIXME: 0
- `panic!`: 1 (intentional — `StorageErrorPolicy::FailWorkflow`, but see C1)
- Public items missing rustdoc: estimated 18 (most concentrated in `otel.rs`
  span model: `OtelSpan` fields, `OtelValue` variants, `OtelStatusCode`,
  `OtelSpanEvent`; plus `TraceContext::workflow`/`child` and several
  `TraceConfig` fields).
- Cargo features: `postgres` (pulls sqlx, no consumer)

## Recommendations (prioritized)

1. **Fix C1 (FailWorkflow panic in drain task)** — the silent-event-loss
   failure mode contradicts the explicit "preserve arrival order"
   invariant the drain task was built for. Smallest patch: remove the
   variant from the public API or panic at the `on_event` sync boundary.
2. **Land M1 + M2 together** — bounded channel for drain task + per-exporter
   timeout/queue. Together these make tracing safe to enable in
   production without risking ingest OOM or sink-induced stalls.
3. **Reconcile M3 + M4 + M5** — pick one of: (a) ship a real OTLP HTTP
   exporter with W3C-compliant random trace IDs and `traceparent`
   ingestion, or (b) downgrade the CLAUDE.md "production-ready OTel"
   claim to "OTel schema, BYO transport" and document the homemade ID
   scheme as intentional (and its tradeoffs). Today the documentation
   overstates capability.
4. **Address M6 (file backend hardening)** — `0o600` permissions, optional
   fsync, wire `delete_old_traces` to the existing `agentflow cleanup`
   command. Cheapest concrete security improvement.
5. **Expand redaction defaults (M8) and text redaction patterns (M9)** —
   ship a more aggressive default key list and fix the URL-query /
   header-continuation cases. Add a property test that fuzzes plausible
   secret shapes against the redactor.
6. **Tighten the captured-IO size limit (M10)** and fix the retry/loop
   node-row leak (M11). Both are small but eliminate silent failures
   that will surface as "trace looks wrong" support tickets.
7. **Long term: implement `PostgresTraceStorage`** (m5) or remove the
   `postgres` feature so the dependency footprint matches the actual
   capability. Add a `SqliteTraceStorage` as the natural midpoint
   between file + Postgres backends; the schema constant is already in
   place.

End of report.
