# Yanshi Workspace Audit — 2026-05-24

**Auditor**: Claude (16 parallel deep-audit agents, one per crate)
**Scope**: 15 Rust crates + 1 TypeScript SPA, all source under `yanshi-*/src/`, tests, examples, migrations, configs.
**Dimensions per crate**: code quality + tests, architecture + docs, security + productization, performance + dependencies.
**Method**: each crate report lives at `docs/audit/<crate>.md` and lists findings with `file/path.rs:LINE` references, severity (CRITICAL / MAJOR / MINOR), what / why-it-matters / fix, plus metrics and prioritized recommendations.

## Per-crate finding counts

| Crate | Layer | Stability | C | M | m | Hot spot |
|---|---|---|---|---|---|---|
| [yanshi-core](yanshi-core.md) | L1 | stable | 0 | 7 | 10 | Non-deterministic topo sort, orphan `robustness.rs`, retry root-cause loss |
| [yanshi-nodes](yanshi-nodes.md) | L2 | stable | 3 | 9 | 14 | SSRF / path traversal / silent fake-data fallback in text_to_image |
| [yanshi-llm](yanshi-llm.md) | L2 | stable | 2 | 7 | 12 | Google API key leaks via `reqwest::Error::to_string()`; no HTTP timeout |
| [yanshi-tools](yanshi-tools.md) | L2 | stable | **6** | 9 | 12 | Sandbox: shell metachar bypass, seccomp `openat` gap, SBPL overbroad |
| [yanshi-mcp](yanshi-mcp.md) | L2 | beta | 2 | 6 | 11 | No JSON-RPC id correlation; stderr piped + never drained → deadlock |
| [yanshi-rag](yanshi-rag.md) | L2 | beta | 1 | 6 | 12 | OpenAI batch sizing 150× under-batch; ONNX UTF-8 boundary risk |
| [yanshi-memory](yanshi-memory.md) | L2 | stable | 2 | 9 | 10 | No SQLite WAL/busy_timeout; malformed `sqlite://` URL builder |
| [yanshi-agents](yanshi-agents.md) | L3 | stable | 0 | 5 | 12 | `expect()` in batch dispatch; PlanExecute ignores `token_budget` |
| [yanshi-skills](yanshi-skills.md) | L3 | beta | 0 | 4 | 10 | Marketplace: path traversal in knowledge/mcp; fake signature verifier |
| [yanshi-harness](yanshi-harness.md) | L3 | **beta** | 2 | 7 | 12 | `seq` namespace split breaks frozen monotonic envelope; no redaction in approval payloads |
| [yanshi-cli](yanshi-cli.md) | L3 | stable | 0 | 6 | 12 | `audio asr --prompt` silently writes file (positional-arg mismatch); no Ctrl-C |
| [yanshi-tracing](yanshi-tracing.md) | L4 | stable | 1 | 11 | 12 | Drain task panic → silent event loss; OTLP exporter has no transport/TLS/auth |
| [yanshi-server](yanshi-server.md) | L4 | beta | 3 | 7 | 13 | Tenant_id from query/body overrides header; Worker PSK non-const-time compare |
| [yanshi-db](yanshi-db.md) | L4 | stable | 1 | 5 | 12 | `SkillInstallRepo::list()` missing tenant filter |
| [yanshi-worker](yanshi-worker.md) | L4 | beta | 1 | 6 | 10 | gRPC channel has no TLS / no auth; no reconnect; no signal handler |
| [yanshi-ui](yanshi-ui.md) | L4 | alpha | 2 | 6 | 10 | Bearer token in localStorage; `EventSource` can't send Authorization → silent polling fallback |
| **TOTAL** | | | **26** | **110** | **184** | |

## Cross-cutting themes

These are recurrences that show up across multiple crates and should be addressed by horizontal sweeps rather than per-crate fixes.

### 1. Multi-tenant boundary is "soft hint", not a security boundary
- `yanshi-server`: 3 critical paths (`list_runs ?tenant_id=`, `list_harness_sessions` ignores extension, all three submit handlers take tenant from body).
- `yanshi-db`: `SkillInstallRepo::list()` missing filter; `mcp_sessions` has no `tenant_id` column; `events.list_after` / `harness_events.list_after` ignore tenant entirely.
- Combined: a single shared bearer token + tenant taken from caller-supplied input means a holder of any token can read or submit across tenants.

### 2. Sandbox / security defaults are not fail-closed
- `yanshi-tools`: ShellTool default no-op sandbox + raw `sh -c` → trivial allow-list bypass via `;`. Linux seccomp filter does not actually block `openat(O_CREAT)`. macOS SBPL grants `/Library` + `/private/etc` reads. `SandboxPolicy.allowed_paths` defaults to permissive-when-empty (opposite of `allowed_commands`).
- `yanshi-nodes`: `FileNode` and `HttpNode` exist in parallel to the sandboxed `yanshi-tools::FileTool` / `HttpTool` but enforce nothing (no path traversal guard, no SSRF guard, no timeout).
- `yanshi-skills`: `os_sandbox` defaults to `false` for `shell` / `script`; marketplace `ChecksumSha256SignatureVerifier` is a self-checksum, not a signature.

### 3. Secret / PII leakage paths
- `yanshi-llm`: Google `?key=<API_KEY>` in URL surfaces in `LLMError` messages and logs.
- `yanshi-harness`: `ApprovalRequest.params_summary` / `ToolCallRequestedPayload.params_summary` embed raw `params.clone()` without invoking `yanshi-tracing::redaction`, despite a "MUST avoid embedding secrets" docstring.
- `yanshi-mcp`: stdio subprocess inherits full parent env including secrets — no `env_clear`.
- `yanshi-llm`: full prompt/response logged at `DEBUG` by default — PII risk.
- `yanshi-ui`: API token persisted to `localStorage` while UI label claims "not persisted".

### 4. `expect()` / `unwrap()` in production code violates project rule (user's global CLAUDE.md)
- `yanshi-llm`: 6 `HeaderValue::from_str(api_key).expect(...)` panic sites — `\n` in `.env` panics the runtime.
- `yanshi-agents`: `expect("every prepared call must have an output...")` in batch dispatch (`react/agent.rs:2073`); `Blackboard.write_internal` uses `.expect("blackboard version poisoned")`.
- `yanshi-cli`: 2 `Mutex::lock().unwrap()` in `commands/eval.rs`.
- `yanshi-nodes`: ~16 production unwraps; `template.rs` global `Mutex<Tera>` will poison-panic on `.lock().unwrap()`.
- `yanshi-core`: `ScopedPermit::Drop` calls `tokio::spawn` → panics if dropped outside a runtime.
- `yanshi-rag`: ~12 non-test occurrences (mostly defensible `expect()` on compile-time regex, but some unprotected).

### 5. No graceful shutdown / signal handling
- `yanshi-server`: `axum::serve(...).await` with no signal handler; spawned run/session tasks dropped on SIGTERM.
- `yanshi-cli`: no `tokio::signal::ctrl_c()` handler anywhere — `workflow run` / `harness run` / `skill chat` exit with no trace flush.
- `yanshi-worker`: cancellation primitive exists but `main.rs` never installs the signal hook; `run_forever` aborts on first transport error.

### 6. Connection robustness gaps
- `yanshi-memory`: no SQLite WAL / busy_timeout / foreign_keys on any of 4 backends → concurrent writes hit `SQLITE_BUSY`.
- `yanshi-db`: connection pool lacks `test_before_acquire` / `max_lifetime` (cloud LB reaping issue).
- `yanshi-worker`: no reconnect; `Mutex<Grpc<Channel>>` serializes all RPCs (concurrency hard-pinned to 1 regardless of `free_slots`).
- `yanshi-mcp`: transport `Arc<Mutex<Box<dyn Transport>>>` serializes every call → defeats parallel-tool-call dispatcher when tools are MCP-backed.

### 7. Productization claims advertised but not implemented
- `yanshi-tracing`: OTLP exporter has no transport / TLS / auth; `OtelSpanSink` has no non-test impl. OTel `trace_id` / `span_id` use FNV hash (violates W3C).
- `yanshi-worker`: gRPC worker↔server has no TLS / no auth; PSK/JWT admission policy is configured server-side but never enforced on the wire.
- `yanshi-harness`: `tracing_bridge` advertises `YANSHI_TRACE_DIR` convention but only returns a `JsonlEventSink` — does not bridge into `yanshi_tracing::ExecutionTrace`, even though CLAUDE.md treats P-H.5 as closed.
- `yanshi-rag`: CLAUDE.md claims StepFun embedding support — only OpenAI + ONNX implemented.

### 8. Docs ↔ reality drift
- `yanshi-db`: CLAUDE.md says "Eight-table schema" / 8 repos — actual count is 9 (missing `user_preferences`); `lib.rs` / `models.rs` / `repo.rs` still say "six tables".
- `yanshi-server`: CLAUDE.md says "Real Flow runner replacing StubExecutor lands in v0.4.0" — actually landed; `FlowRunExecutor` is the default.
- `yanshi-nodes`: documented per-modality feature gates (`asr`, `tts`, `text_to_image`, etc.) don't exist — all heavy nodes compile unconditionally.
- `yanshi-cli`: `plugin` and `rag` are feature-gated and NOT in `default = []`, contradicting CLAUDE.md / RoadMap.md.
- `yanshi-mcp`: CLAUDE.md says "adapter into yanshi-tools::ToolRegistry" — adapter actually lives in `yanshi-skills/src/mcp_tools.rs`.
- `yanshi-nodes`: `NodeFactory` trait is declared + exported but has zero implementations workspace-wide; CLI uses a parallel API in `yanshi-cli/src/executor/factory.rs` — dead public surface.

### 9. Concurrency anti-patterns
- `yanshi-core`: `topological_sort` iterates a `HashMap` for queue init → non-deterministic node order, breaks trace replay reproducibility.
- `yanshi-tools`: `openai_tools_array` returns tools in non-deterministic `HashMap` order.
- `yanshi-memory`: `add_message` takes `&mut self` despite the pool being concurrent-safe → blocks H3 parallel tool-call memory writes.
- `yanshi-mcp`, `yanshi-worker`: Mutex-wrapped transports as above.
- `yanshi-server`: `LiveHarnessExecutor` spawns 1 OS thread per concurrent harness session (workaround for `HarnessRuntime: !Sync`).

### 10. Latent silent-bug pockets
- `yanshi-nodes`: `arxiv.rs:252` uses `r"\\begin{document}"` (literal `\\` two backslashes) — main-content detection never triggers.
- `yanshi-nodes`: `src/nodes/while.rs` is a 0-byte file not in `mod.rs`.
- `yanshi-cli`: `audio asr --prompt VALUE` silently writes transcription to path `VALUE` (positional arg mismatch).
- `yanshi-llm`: Anthropic streaming terminates on `content_block_stop` (per-block) instead of `message_stop` → multi-block responses truncated.
- `yanshi-llm`: streaming drops native tool-call deltas (`OpenAIStreamingResponse` ignores `delta.tool_calls`).
- `yanshi-memory`: `row_to_message` silently fabricates fresh UUIDs/timestamps on parse error → breaks `AgentNodeResumeContract` keys.
- `yanshi-tracing`: `next_event_seq` retry/loop node lookups leak phantom "running" rows.

## Suggested fix order

The order below is by **production risk × blast radius**, not by raw severity count.

### Wave 1 — production-blocking security (do before any v0.4 / v1.0-rc tag)
1. **Multi-tenant boundary** (`yanshi-server`, `yanshi-db`): require tenant from authenticated extension only; remove every body / query tenant source; backfill `mcp_sessions.tenant_id`; add `tenant_id` filters to every `list_*` repo method; defense-in-depth tests.
2. **Worker channel auth** (`yanshi-worker`): enforce `AuthenticatedControlPlane` on the gRPC adapter; require TLS in production profile.
3. **Tools sandbox correctness** (`yanshi-tools`): shell metachar quoting / argv-only mode; seccomp `openat(O_CREAT|O_WRONLY)` filter; tighten SBPL allowed reads; flip `SandboxPolicy.allowed_paths` to fail-closed-when-empty.
4. **Harness redaction** (`yanshi-harness`): invoke `yanshi-tracing::redaction` on every `params_summary` field; merge `seq` namespace (single source of truth).
5. **LLM key + timeout** (`yanshi-llm`): move Google key from URL to header; default HTTP timeout on all providers.
6. **UI auth surface** (`yanshi-ui`): move bearer to session cookie OR swap `EventSource` for `fetch + ReadableStream`; sync UI labels with reality.

### Wave 2 — correctness / data integrity
7. **SQLite production hardening** (`yanshi-memory`): WAL + busy_timeout + foreign_keys on every backend; fix `sqlite://` URL builder.
8. **Drain task survival** (`yanshi-tracing`): drain task must catch panics and bound the channel; W3C-compliant trace/span IDs.
9. **OTLP exporter actually works** (`yanshi-tracing`): wire OTLP transport / TLS / auth or drop the claim from CLAUDE.md.
10. **Deterministic ordering** (`yanshi-core`): `BTreeMap` (or stable insertion order) in `topological_sort`; same for `openai_tools_array`.
11. **`expect()` / `unwrap()` sweep** (workspace-wide): convert per the global rule (no-panic in production); start with `yanshi-llm` header builder, `yanshi-agents` batch dispatch, `yanshi-cli` eval.

### Wave 3 — productization hygiene
12. **Graceful shutdown** (server / cli / worker): single `tokio::signal::ctrl_c` + drain pattern.
13. **MCP robustness** (`yanshi-mcp`): JSON-RPC id correlation; drain stderr; `env_clear` for subprocess; per-request oneshot to remove Mutex serialization.
14. **Marketplace integrity** (`yanshi-skills`): replace `ChecksumSha256SignatureVerifier` with real Ed25519 or minisign; path-traversal guard on knowledge/mcp args; max-bytes + ETag on remote fetch.
15. **CLI silent bugs** (`yanshi-cli`): fix `audio asr` positional mismatch; surface or remove discarded `llm chat` flags; align `plugin` / `rag` feature defaults with CLAUDE.md claims.

### Wave 4 — docs ↔ reality reconciliation
16. **CLAUDE.md sweep** — update table counts (db 8→9), Real Flow runner status, mcp adapter location, `yanshi-nodes` per-modality feature gates, `yanshi-rag` StepFun claim.
17. **RoadMap.md sweep** — re-mark items that are advertised closed but have implementation gaps (P-H.5 tracing bridge, OTLP exporter, worker auth).

## Positive observations (workspace-wide)

- **Zero `unwrap()/expect()` in production code** in `yanshi-core`, `yanshi-memory`, `yanshi-db`, `yanshi-worker`, `yanshi-tools` non-test paths — the project rule lands well in the base layer.
- **`.no_proxy()` discipline** is exemplary in `yanshi-llm` (all 60+ test clients), `yanshi-cli`, `yanshi-tools` (HttpTool tests).
- **Frozen-fixture tests** in `yanshi-harness` and JSON-fixture wire-contract tests in `yanshi-mcp` are reference-quality patterns other crates should adopt.
- **`yanshi-rag` eval harness** (`src/eval/`) — log-space binomial p-value, schema-stable serde, retriever-agnostic, bundled dataset.
- **Cross-provider consistency suite** in `yanshi-llm::provider_consistency` (2129 LOC) — strong regression coverage.
- **Cancellation primitives** (`yanshi-worker` `WorkerCancellationToken` + `tokio::select! biased`) are well-designed even where not yet plumbed end-to-end.
- **L1–L4 layering** is clean in every Rust crate (no upward dependencies); the one exception is `yanshi-worker → yanshi-server` (L4↔L4 inversion).
