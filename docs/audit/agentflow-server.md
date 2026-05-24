# Audit: agentflow-server

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-server/
**Crate version**: 0.1.0 (workspace 0.2.0+, targeting v0.3.0)
**Layer**: L4 (Operations / Productization, security-critical)
**Stability tier**: Beta (REST envelopes + SSE per `docs/STABILITY.md`); scheduler subsystem Experimental.

## Scope summary

Axum gateway exposing the workflow surface (`/v1/runs`, SSE, skills), the Harness Mode surface (`/v1/harness/sessions`, approvals, SSE), embedded UI (`/ui/...`), preferences, diagnostics, Prometheus `/metrics`, and the distributed-worker control plane (gRPC + admission). 18 source files / ~9 500 LOC excluding tests; 18 integration-test files / ~5 700 LOC.

Routing is centralized in `lib.rs::create_router`. State is `AppState` (sqlx pool + repos + auth + skills + harness/worker brokers + approval registry + live-state registry + security profile defaults). Bearer middleware (`auth::require_bearer_token`) is applied to the entire `/v1/*` subtree when `AppState::auth` is `Some`; health / `/metrics` / UI bypass auth. Tenant binding (`tenant::extract_tenant_id`) injects a `TenantId` extension into every `/v1/*` request, defaulting to `"default"` when the header is absent.

Real Flow runner is implemented (`runs::FlowRunExecutor`, `runs.rs:244-275`) wrapping `agentflow_cli::executor::build_flow_from_yaml`; `StubExecutor` is now a test-only fallback. The Harness `LiveHarnessExecutor` (`harness_live.rs`) wraps `ReActAgent` + `HarnessRuntime` and is wired by `serve::run`. CLAUDE.md's "Real Flow runner replacing StubExecutor lands in v0.4.0" is stale — it landed already.

## Findings

### CRITICAL (especially security)

- [C1] Cross-tenant data exposure via `?tenant_id=` query parameter on `GET /v1/runs` and `GET /v1/harness/sessions` — `agentflow-server/src/runs.rs:614-633` and `agentflow-server/src/harness.rs:481-489`.
  **What**: `list_runs` accepts `ListRunsQuery::tenant_id` and uses it verbatim, overriding the header-bound `TenantId` extension (the test `list_runs_query_param_overrides_header` at `tests/e2e_runs.rs:502` documents this as "query wins for backward compat"). `list_harness_sessions` is worse — it does not extract `Extension<TenantId>` at all and uses only the query param defaulting to `"default"`. Any authenticated client (single shared bearer token gates the whole gateway) can list rows from any tenant by passing `?tenant_id=<victim>`.
  **Why it matters**: The other `:id`-bound endpoints (P2.6 work, commits `fa3f5e5` / `55a5fa9` / `60b3987`) all clamp tenant via header-bound 404, but the list endpoints leak the existence + payload + status + workflow body of every tenant's runs/sessions. Combined with single-shared-token auth, the multi-tenant story is functionally a soft hint, not a security boundary. A SaaS deployment leaks competitor workflow bodies.
  **Fix**: For list endpoints, treat the header tenant as authoritative once auth is enabled (`production` profile). Either (a) drop the query param entirely and always use the extension, or (b) reject the request when `?tenant_id=` differs from the extension under `production` profile. Document the relaxed behavior for `local`/`dev`. Add `list_harness_sessions` extension binding either way — its current shape can't even consult the header.

- [C2] Write-side tenant spoofing on POST `/v1/runs`, `/v1/harness/sessions`, and `/v1/skills/{name}:run` — `agentflow-server/src/runs.rs:505`, `agentflow-server/src/harness.rs:427`, `agentflow-server/src/skills.rs:198`.
  **What**: All three submit handlers ignore the `TenantId` extension and accept `tenant_id` from the request body, defaulting to `"default"`. `submit_run`, `submit_harness_session`, and `run_skill` never look at `Extension<TenantId>`.
  **Why it matters**: An authenticated client can plant rows + events + on-disk run dirs in any other tenant's namespace, causing data poisoning, quota / retention exhaustion, fake event injection into another tenant's SSE stream, or storage bills under another tenant. The on-disk `run_dir_for_run` path is also unscoped, so the write hits the other tenant's filesystem footprint too.
  **Fix**: Drop the body `tenant_id` entirely (it's redundant with the header). When backwards compat is required, validate that body matches extension and reject with 403 otherwise. The submit handlers must use `Extension<TenantId>` as the source of truth.

- [C3] PSK worker admission uses non-constant-time comparison — `agentflow-server/src/scheduler/admission.rs:193`.
  **What**: `valid_tokens.contains(presented)` compares the worker-presented PSK to the rotation table via `HashSet::contains`, which delegates to `String::eq` (data-dependent early-out). The HTTP bearer path correctly uses `constant_time_eq` (`auth.rs:119`), but the PSK path does not.
  **Why it matters**: An attacker who can observe network latency (same datacenter, shared host) can brute-force a worker PSK byte-by-byte. The wider distributed-worker promise is "experimental," but PSKs are described as a hardening step in `docs/DISTRIBUTED.md`. This is an inconsistency with the explicit constant-time pattern already established in this crate.
  **Fix**: Iterate over the rotation set with `constant_time_eq` on every entry (always run to completion regardless of mismatch). The `subtle` crate or the existing inline helper both work; lift `auth::constant_time_eq` to a crate-internal util.

### MAJOR

- [M1] Configured `max_request_body_bytes` is never applied to the router — `agentflow-server/src/lib.rs:233-234` + `lib.rs:469-538`.
  **What**: `server_security_defaults_from_env` reads `AGENTFLOW_MAX_REQUEST_BODY_BYTES` into `defaults.request_limits.max_request_body_bytes`, and the security profile defaults it to 10 MiB (production). But `create_router` only consults `max_workflow_submit_bytes` (POST `/v1/runs`, POST `/v1/harness/sessions`) and `max_skill_run_bytes` (POST `/v1/skills/:name_run`). All other POST/PUT endpoints — `POST /v1/runs/:id:cancel`, `POST /v1/harness/sessions/:id` (cancel/resume), `POST /v1/harness/sessions/:id/approvals/:request_id`, `PUT /v1/preferences` — fall back to axum's default 2 MB cap.
  **Why it matters**: (a) the documented `AGENTFLOW_MAX_REQUEST_BODY_BYTES` knob is silently ignored, (b) operators who set the workflow limit higher (say 100 MB) leak that limit onto handlers that don't expect it because they're under the default cap — but a `local` / `dev` profile with 100 MB workflow gives no global ceiling. Inconsistent DoS surface.
  **Fix**: Apply a global `DefaultBodyLimit::max(max_request_body_bytes)` to the router root, then layer per-route `DefaultBodyLimit::max(...)` on the workflow/skill endpoints. Document that the per-route limit overrides the global.

- [M2] No graceful shutdown — `agentflow-server/src/serve.rs:436-446`.
  **What**: `run()` calls `axum::serve(listener, app).await` with no `with_graceful_shutdown` signal handler. SIGTERM tears the listener down immediately; in-flight runs/sessions (spawned via `tokio::spawn` from `submit_run`/`submit_harness_session`) are dropped mid-execution. The background cleanup loop is a `tokio::spawn` that never gets a chance to drain.
  **Why it matters**: Kubernetes pod rollouts / autoscaler scale-downs lose data: runs in `Running` state with no terminal status row, SSE subscribers see abrupt disconnects without `stopped` events, harness sessions stranded in `Running` forever. The CLAUDE.md docs claim "Graceful shutdown: drain in-flight runs" is a productization goal — currently it is not honored.
  **Fix**: Use `axum::serve(...).with_graceful_shutdown(shutdown_signal())` where `shutdown_signal` listens for SIGTERM/SIGINT. Track spawned run/session `JoinHandle`s in `AppState` (mirror `RunCancellationRegistry`), and on shutdown signal: (1) stop accepting new POSTs, (2) wait bounded time for in-flight to complete, (3) flip incomplete rows to `Cancelled` with reason "server_shutdown".

- [M3] No rate limiting or per-tenant DoS protection.
  **What**: A single authenticated client can POST to `/v1/runs` repeatedly; each POST `tokio::spawn`s a background executor. The only backpressure is the sqlx pool (8 connections) and OS memory. No `tower::limit::ConcurrencyLimit`, `RateLimitLayer`, or per-tenant `Semaphore`.
  **Why it matters**: Trivially exhaust the DB connection pool, fill `~/.agentflow/runs/`, exhaust LLM provider quotas (since the FlowRunExecutor calls real models), or stall the cleanup loop. The harness executor spins up an OS thread per session (`harness_live.rs:301`'s `spawn_blocking` strategy) — concurrent submissions can fork thousands of threads.
  **Fix**: Add `tower::limit::ConcurrencyLimitLayer` per-route on the POST submit handlers. Track per-tenant concurrent runs in a process-local semaphore map. Document the relationship between the sqlx pool size and the global concurrency cap. Consider a `tower_governor` per-IP rate limit for the unauthenticated `local` profile.

- [M4] No OpenAPI / API specification document.
  **What**: There is no `agentflow-server/openapi.yaml` or equivalent. Route shapes are documented in module-level rustdoc + `docs/HARNESS_MODE.md` (Harness only) + a few entries in `docs/STABILITY.md`. The unified error envelope is documented in `docs/DEPLOYMENT.md` and `src/error.rs`, but there is no machine-readable spec.
  **Why it matters**: Beta-tier wire promise (`docs/STABILITY.md:69,99`) is hard to verify against drift; SDK generation for users requires hand-tracing handler signatures. The compatibility tests in `tests/server_envelope_compat.rs` and `tests/fixtures/rest_envelopes/` substitute for the spec but are not a contract clients can read.
  **Fix**: Add an `openapi.yaml` at the crate root, generated or hand-written, covering all `/v1/*` routes + envelopes + status codes. Wire it into CI so route additions must update the spec. Consider `utoipa` for inline annotation if hand maintenance is a concern.

- [M5] `prometheus_metrics` endpoint executes 3 DB queries on every scrape with no caching or rate limiting — `agentflow-server/src/lib.rs:342-446`.
  **What**: `refresh_scrape_time_gauges` runs (a) `SELECT status, COUNT(*) FROM harness_sessions GROUP BY status`, (b) `SELECT 1` health probe, (c) `SELECT tenant_id, COUNT(*) FROM runs WHERE status IN ('queued','running') GROUP BY tenant_id` on every `/metrics` request. The `/metrics` endpoint is unauthenticated.
  **Why it matters**: A scraper misconfigured to poll every second (or an attacker hitting `/metrics` rapidly since it has no auth) can pin the read pool. At typical Prometheus scrape intervals (15-30s) the cost is fine, but high-frequency scraping or a small adversarial loop becomes a free DoS vector against the DB. The cardinality on `agentflow_state_size_bytes{run_id}` (line 443-445) emits one gauge per active run — unbounded per scrape.
  **Fix**: Cache the scrape-time gauges for ~5 seconds inside `metrics::observe_*`. Either gate `/metrics` behind auth or restrict via a unix-socket / internal listener. Cap the per-scrape `run_id` cardinality at a documented ceiling so the runaway scrape doesn't blow up Prometheus memory.

- [M6] `LiveHarnessExecutor` spawns one OS thread per concurrent harness session — `agentflow-server/src/harness_live.rs:297-318`.
  **What**: `run_harness_blocking` uses `tokio::task::spawn_blocking` + `Runtime::new_current_thread()` per session to work around `HarnessRuntime: !Sync`. Comment says "one OS thread per concurrent harness session, which is acceptable for now."
  **Why it matters**: 1 000 concurrent sessions → 1 000 OS threads. The default `tokio` blocking pool has 512 threads; once exhausted, sessions queue silently and the HTTP submit returns 200 while the agent never starts. Combines with M3 (no rate limiting) to make a "submit 10 000 sessions" attack trivially fatal.
  **Fix**: As the comment notes, the right fix is upstream: add `Sync` to `AgentRuntime` (or thread `&mut self` through `HarnessRuntime::run`). Short-term: track a hard concurrent-session cap in `AppState` with a `Semaphore`; reject new submissions with 503 + Retry-After when saturated. Increase the blocking pool size in `serve.rs`.

- [M7] `whoami` always returns `authenticated: true` regardless of actual auth state — `agentflow-server/src/lib.rs:615-618`.
  **What**: When `AppState::auth` is `None` (local dev, tests), the bearer middleware isn't attached. A `GET /v1/whoami` succeeds and returns `{authenticated: true}` even though no token was checked. Worse, the handler hardcodes `true` — it doesn't read any auth context.
  **Why it matters**: Misleading — operators inspecting a dev/local deployment see "authenticated: true" and assume the bearer gate is in effect. Should reflect whether the request was actually authenticated. Minor on its own; promoted to MAJOR because it's the documented smoke endpoint per the comment "gives the auth middleware something concrete to gate".
  **Fix**: Read the auth state from `AppState` (or a dedicated `Authenticated` request extension set by the middleware) and reflect it accurately.

### MINOR

- [m1] `ServerApprovalProvider::request` uses `.to_std().unwrap_or_default()` on the deadline math — `agentflow-server/src/harness_approval.rs:199`. If `expires_at` is in the past, the timeout becomes `Duration::ZERO`, which `.filter(|d| !d.is_zero())` correctly falls through to `default_timeout`. The `unwrap_or_default` is harmless but obscures intent — replace with explicit branch.
- [m2] `next_event_seq` (`runs.rs:733-743` and `harness.rs:824-837`) fetches up to 10 000 rows just to find `max(seq)`. Use a dedicated `repos.events.max_seq(run_id)` query (already exists for harness — `harness_events.max_seq`). Otherwise this is O(N) per cancel call and pages 10 000 rows into memory on long-lived runs.
- [m3] `harness.rs:957` `serialise_event` uses `unwrap_or_else(|_| "{}".to_string())` to swallow `serde_json::to_string` errors. Same shape at `events_stream.rs:288`. JSON serialization of a `StreamedEvent` can't fail given the input types (all `Serialize`-derived), so this is correct, but a `tracing::warn!` on failure would help future variants where serialization can fail.
- [m4] `stream_harness_events` (`harness.rs:846-904`) and `stream_events` (`events_stream.rs:206-268`) backfill cap at 1 000 events with a "let the live stream catch the tail" comment, but the live stream's broadcast capacity is 256 (`harness.rs:56`, `events_stream.rs:38`). A subscriber reconnecting after a long disconnect against a high-rate run will see a gap if the broadcast buffer is rotated before the SSE stream finishes the backfill page. Document or fix.
- [m5] `LiveHarnessExecutor` carries an empty `ToolRegistry::new()` — `harness_live.rs:345`. The whole approval pipeline is wired but there are no tools to approve. This is documented in the comment, but means the live harness path on the server cannot meaningfully exercise hooks beyond the smoke level. Track skill/MCP tool loading as a follow-up.
- [m6] `looks_like_token` heuristics in `preferences.rs:134-170` are good but case-mismatch sensitive for `Bearer ` (matches lower-case only at line 143). Bearer prefix should be case-insensitive (`s.to_lowercase().starts_with("bearer ")` already happens, so this is actually fine — disregard if accurate; double-check).
- [m7] `process_memory_bytes` (`lib.rs:455-467`) returns `None` on macOS/Windows. The caller emits `0`, which the dashboard renders as red. Add a darwin path via `mach_task_basic_info` or document the limitation in the dashboard.
- [m8] `from_env_treats_empty_as_unset` (`auth.rs:142-163`) uses `std::env::set_var` inside `unsafe`. Acceptable for the test annotation, but creates a process-wide side effect that other parallel tests share — flaky if any test reads `AGENTFLOW_API_TOKEN` concurrently.
- [m9] No tests for `/v1/preferences` token-shape rejection at the route layer — only the helper `looks_like_token` is unit-tested (`preferences.rs:188-217`). Route-level integration test missing in `tests/preferences_route.rs` (8 tests cover other shapes but not the secret-screen 400).
- [m10] `error.rs:30-41` defines `ErrorEnvelope` / `ErrorBody` as private; the wire shape relies on hand-written tests. Worth promoting these to `pub` and serde-stamping them so external SDK consumers can pin the schema. The `details: Option<Value>` field is reserved but never populated — either document the reservation or wire it into the `BadRequest` path (validation errors).
- [m11] `read.pool()` is used for the scrape-time gauges (good — keeps reads on the replica) but the cleanup sweep (`cleanup.rs`) uses the primary `db.pool` for SELECT queries (`cleanup.rs:151,156,158`). Routing the read-side `list_terminal_runs` / `preview_*` to the replica would lift write-pool pressure during retention sweeps.
- [m12] `events_stream.rs:288` (`serialise_event`) returns `"{}"` on JSON encoding failure; same in `harness.rs:957`. Document the fallback or upgrade the encoding to be infallible (the input is a known `Serialize` derive).
- [m13] CLAUDE.md is stale: "Real Flow runner replacing StubExecutor lands in v0.4.0" is wrong — `FlowRunExecutor` exists at `runs.rs:244-275` and is the default `executor` since `runs.rs:815`. Update CLAUDE.md.

### POSITIVE OBSERVATIONS

- Bearer token comparison uses an inline constant-time helper (`auth.rs:109,119`), with a dedicated unit test (`auth.rs:135`). Constant-time inequality is correctly length-prefixed.
- The production security profile fails closed: `resolve_auth_config(Production, None)` returns `Err(MissingRequiredToken)` (`auth.rs:60`), and the startup readiness probe in `build_startup_report` (`serve.rs:242`) escalates this to `ServeReadiness::Fail`.
- Tenant boundary enforcement on every `:id`-bound endpoint is consistent: `get_run` / `cancel_run` / `get_run_resume_plan` / `list_events` / `stream_events` / `get_harness_session` / `cancel_harness_session` / `resume_harness_session` / `stream_harness_events` / `list_harness_events` / `list_pending_approvals` / `decide_approval` all use the "404 cross-tenant" pattern with consistent comment markers (`P2.6 tenant boundary`).
- The unified error envelope (`error.rs`) is wired through every handler via `ApiError` + `JsonReq<T>`. Malformed JSON bodies map to envelope-shaped 400s, not axum's default plain-text rejection (`json_rejection_to_api_error`).
- Bearer middleware mounts only on `/v1/*`; `/health`, `/health/live`, `/health/ready`, `/metrics`, and `/ui/*` bypass auth as intended (kubelet probes, scrapers, embedded SPA bootstrap).
- The harness approval flow keys parked oneshots on `(session_id, request_id)` *and* checks `session.tenant_id != tenant.as_str()` before resolving (`harness_approval.rs:289-308`), so an attacker who somehow guesses a request_id cannot resolve another tenant's approval through the HTTP decide route.
- Workflow / harness brokers use a `finalise_with_grace` pattern (`events_stream.rs:126`, `harness.rs:156`) that defers channel teardown so terminal events drain to SSE subscribers — explicitly tested.
- SSE backfill ordering is tested directly (`tests/sse_robustness.rs:113-227`): replay, no-history-against-completed-run, and after-seq-above-all paths.
- The token-shape screen on `PUT /v1/preferences` (`preferences.rs:134-170`) is defense-in-depth — catches Bearer / OpenAI / Anthropic / GitHub / long hex / base64-shaped strings before persistence.
- `JsonReq<T>` preserves the 413 status from `DefaultBodyLimit` rejections (`error.rs:156-158`) so clients can branch on payload-size vs. malformed-body causes. Explicitly contract-tested at `tests/auth_and_errors.rs:206,226`.
- Cleanup sweep honors per-run retention overrides via `GREATEST(global, COALESCE(override, 0))` SQL (`cleanup.rs:207-211`), which is the right semantic for "additive only" retention pinning.
- Live harness executor emits a synthetic `stopped` event on failure paths (`harness_live.rs:191,199-236`) so the H0 contract's "terminal event always present" promise is honored even when the inner runtime errors before completing.

## Metrics

- Source files: 22 (excluding tests / examples)
- Lines of code: ~9 500 (src/, excluding tests)
- Routes registered:
  - Health (no auth): `GET /health`, `GET /health/live`, `GET /health/ready`, `GET /metrics` — 4 routes.
  - Workflow surface: `GET /v1/whoami`, `GET|POST /v1/runs`, `GET|POST /v1/runs/:id`, `GET /v1/runs/:id/resume-plan`, `GET /v1/runs/:id/events/history`, `GET /v1/runs/:id/events` (SSE), `GET /v1/skills`, `POST /v1/skills/:name_run`, `GET /v1/diagnostics`, `GET|PUT /v1/preferences` — 11 routes.
  - Harness surface: `GET|POST /v1/harness/sessions`, `GET|POST /v1/harness/sessions/:id`, `GET /v1/harness/sessions/:id/events/history`, `GET /v1/harness/sessions/:id/events` (SSE), `GET /v1/harness/sessions/:id/approvals`, `POST /v1/harness/sessions/:id/approvals/:request_id` — 7 routes.
  - UI / static (no auth): 11 `/ui/*` routes.
- Test files: 18 integration tests + ~22 unit `#[cfg(test)]` modules.
- `unwrap()/expect()` in non-test code: 16 sites, all `Mutex::lock().expect("... poisoned")` panic-on-poisoning idioms or `serve.rs:81`'s `DEFAULT_SERVE_BIND.parse().expect(...)` (constant-string parse, infallible). No data-driven unwrap on user input. Top sites:
  - `runs.rs:190,204,217` — `RunCancellationRegistry` mutex.
  - `events_stream.rs:89,100,113,148` — `EventBroker` mutex.
  - `harness.rs:122,135,148,178` — `HarnessEventBroker` mutex.
  - `harness_approval.rs:93,108,118,138` — `PendingApprovalRegistry` mutex.
  - `serve.rs:81` — infallible constant parse.
- TODO/FIXME/XXX/HACK: 0 in `src/`; 1 in `tests/worker_admission.rs` (referencing a closed TODO).
- Public items missing rustdoc: estimated < 5%. Coverage is excellent — almost every `pub` item has a `///` block, often multi-paragraph. The recent `fa3f5e5` commit explicitly cleaned up rustdoc `-D warnings`.

## Recommendations (prioritized)

1. **Fix C2 / C1 first**: tenant spoofing on submit + list. These are exploitable today by any authenticated caller. Single-shared-token + multi-tenant is incompatible; either drop the multi-tenant claim or enforce header-tenant authoritative on every handler. Tests should be inverted (currently `list_runs_query_param_overrides_header` documents the bug as a feature).
2. **Fix C3**: constant-time PSK comparison in `scheduler::admission`. Mechanical change, no semantic risk.
3. **Wire `max_request_body_bytes` globally (M1)** and add per-route `DefaultBodyLimit` to every POST/PUT handler that currently inherits the 2 MB default.
4. **Add graceful shutdown + spawned-task tracking (M2 + M6)**. The harness OS-thread amplification on top of unbounded concurrency is the most likely production fire.
5. **Add rate limiting / concurrency caps (M3)**, per-tenant where feasible. Bound the harness session concurrent count.
6. **Ship an OpenAPI spec (M4)** alongside the existing fixture-based compat tests. Wire it into CI so new routes can't merge without a spec update.
7. **Gate `/metrics` (M5)** behind auth or bind it to an internal-only listener; cache scrape-time gauges; cap `run_id` cardinality.
8. **Fix `whoami` accuracy (M7)** — small but symbolic.
9. Audit the `next_event_seq` paths (m2) for `max_seq()` replacement to drop the 10 000-row scan.

End of report.
