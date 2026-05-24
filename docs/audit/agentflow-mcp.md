# Audit: agentflow-mcp

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-mcp/
**Crate version**: 0.2.0 (per Cargo.toml)
**Layer**: L2 (Capability Adapter)
**Stability tier**: Beta (server surface explicitly Beta per `src/server.rs` rustdoc; client/transport tier not documented but treated similarly through `tests/server_contracts.rs` pinning)

## Scope summary

`agentflow-mcp` implements Model Context Protocol client + server + transport
for stdio. Wire shape is JSON-RPC 2.0 line-delimited. Three concerns dominate:

1. **Stdio subprocess lifecycle**: spawn / health-check / graceful + forceful
   teardown. Uses `tokio::process::Command` with `kill_on_drop(true)`.
2. **Request/response correlation**: serial in-process Mutex serializes every
   call; there is **no `response.id` validation** — the client returns whatever
   line arrives next on stdout, trusting the order.
3. **Cross-hop W3C `traceparent`** injection into `params._meta.traceparent`
   (the canonical AgentFlow OTel stitching contract for MCP hops).

The crate ships:
- 14 source files (~4.6k LoC core + tests inline), 6 integration test files,
  3 examples, 1 latency benchmark, 1 fixtures dir with 6 server-contract JSON
  fixtures.
- Mock transport for unit testing, real `StdioTransport`, trait `Transport`
  with `TransportType::{Stdio, Http, HttpWithSSE}` (only Stdio implemented).
- Beta server contract pinned by 8 fixture-driven tests.

CLAUDE.md claim "adapter into agentflow-tools::ToolRegistry" — the registry
adapter lives in `agentflow-skills/src/mcp_tools.rs`, not in `agentflow-tools`
or `agentflow-mcp` itself. It exists and works (verified by reference); the
boundary statement in CLAUDE.md is slightly misleading about where the
adapter sits.

## Findings

### CRITICAL

- [C1] No JSON-RPC response-id correlation — `src/client/session.rs:341-344`,
  `src/transport/stdio.rs:280-290`
  **What**: `StdioTransport::send_message` writes a request, then `read_line`s
  the next line off stdout and returns it as the response. The client
  (`MCPClient::send_request`) parses it as `JsonRpcResponse` and dispatches
  without verifying `response.id == request.id`. The protocol allows
  out-of-order responses (responses arrive in arbitrary order on a single
  connection), and even in serial use any unprompted server message
  (notifications, progress events, `notifications/resources/updated`) arriving
  on stdout between request and response will be silently returned as the
  "response" to whatever call was in flight. The first such occurrence
  silently mis-decodes; subsequent calls remain off-by-one for the life of
  the session.
  **Why it matters**: subscribed resources, long-running tool calls with
  `notifications/progress`, and any server that emits log notifications to
  stdout will corrupt the request/response stream. Currently masked because
  the `Arc<Mutex<Box<dyn Transport>>>` serializes calls and the example MCP
  servers tested (`mcp-basic` Python server, line 109-117 of the latency
  benchmark) don't emit unsolicited messages. Any production server that
  follows the MCP spec for resource subscriptions or progress notifications
  will trigger this immediately.
  **Fix**: in `StdioTransport::send_message`, loop on `read_line` and either
  (a) parse each line and dispatch unsolicited notifications via a separate
  channel until a response with the matching id arrives, or (b) restructure
  the transport as a background reader task with `tokio::sync::oneshot`
  channels keyed by request id (the standard JSON-RPC client pattern). The
  current design also blocks (b) being added later as a non-breaking change
  because the `Transport` trait is request-response shaped.

- [C2] Stderr pipe is captured but never drained — `src/transport/stdio.rs:235`
  **What**: spawn config includes `.stderr(std::process::Stdio::piped())`, but
  the child's stderr handle is **never taken** off the `Child` and is never
  read. On Linux the default pipe buffer is ~64KB; any MCP server that logs
  chattily to stderr (which most do — npx-launched servers print install
  output, Python servers print tracebacks, Node servers print startup
  banners) will eventually fill the pipe, block on `write(stderr)`, and hang
  the entire server process. The client then sees read timeouts that look
  like "the server stopped responding" — the root cause is opaque.
  **Why it matters**: this is a guaranteed-eventual hang under load for any
  realistic MCP server. Combined with the missing health check on read
  timeout (the client retries with the same broken process), this can take
  the whole agent loop down.
  **Fix**: either spawn a `tokio::task` that drains stderr to a ring buffer
  (exposed via `last_stderr_lines()` for debugging) and discards the rest, or
  change the spawn to `Stdio::inherit()` / `Stdio::null()` and document the
  trade-off (the spec says servers SHOULD use stderr for logging — inheriting
  is the simplest correct choice). Today the worst of both worlds: captured
  + ignored.

### MAJOR

- [M1] Subprocess env / cwd / fd inheritance is unconfigurable beyond
  additive env — `src/transport/stdio.rs:79-96, 222-232`
  **What**: `StdioTransport::new(command)` + `with_env(map)` is the entire
  surface. `Command::envs(...)` is additive (does not clear inherited env),
  so the spawned MCP server inherits the entire parent env including
  `OPENAI_API_KEY`, `AWS_SECRET_ACCESS_KEY`, and any other secrets present
  in the agent process. There is no `env_clear` knob, no `current_dir`
  knob, no `pre_exec` hook for fd close-on-exec, no `umask` control. The
  spawned process also inherits open file descriptors from the parent
  (default for `tokio::process::Command`).
  **Why it matters**: in a production agent that hosts multiple users'
  workloads (the platform-mode target documented in CLAUDE.md), a third-
  party stdio MCP server gets full read access to the host's secrets via
  env. Concrete bug class: a malicious npm package shipped as an MCP
  server can exfiltrate `OPENAI_API_KEY`. The agent-tools `Tool` /
  `SandboxPolicy` machinery exists in `agentflow-tools` but is not wired
  through `agentflow-mcp`.
  **Fix**: extend `StdioTransport` with `with_clean_env()`,
  `with_env_filter(Fn(&str) -> bool)`, `with_current_dir(PathBuf)`. Default
  the platform-mode adapter (in `agentflow-skills::mcp_tools` or wherever
  the registry adapter lives) to clean env + explicit allowlist, mirroring
  what `agentflow-tools::SandboxPolicy` does for shell tools.

- [M2] HTTP transport advertised in `TransportType` enum but not implemented
  — `src/transport/traits.rs:13-20`, `src/transport/mod.rs:29-35`,
  `Cargo.toml:43-46`
  **What**: `TransportType::Http` and `TransportType::HttpWithSSE` are
  defined as variants of the enum and documented in lib.rs / module docs,
  but no implementation exists. The `Cargo.toml` declares unused features
  `http`, `client`, `server`, `stdio` (none of these gate any `#[cfg]` code
  in the crate today). MCP's stable HTTP+SSE transport is widely used by
  hosted MCP servers (Anthropic, Anthropic-hosted skills, third-party
  cloud servers).
  **Why it matters**: anyone reaching for `TransportType::Http` discovers
  there's no impl. The `supports_server_messages()` default returns `true`
  for `HttpWithSSE`, suggesting the trait already has plumbing for SSE,
  but the trait's `send_message` shape (`async fn send_message(&mut self,
  request) -> Result<Value>`) makes a real SSE impl awkward because SSE
  flows are write-one-read-many.
  **Fix**: either delete the unimplemented variants (breaking — bump
  major) or write the HTTP transport. Roadmap silence on this is itself
  a finding — there is no MCP-related entry in RoadMap.md beyond the
  passing CLAUDE.md mention.

- [M3] Mock-transport `Mutex::lock().unwrap()` panic surface — 14 sites,
  all in `src/transport/mock.rs:62-241`
  **What**: `MockTransport` uses `std::sync::Mutex::lock().unwrap()` in
  every method. This is "test infrastructure", but `MockTransport` is
  `pub` (re-exported as `agentflow_mcp::transport::MockTransport`) and is
  used in production-facing test harnesses across the workspace (e.g.
  `agentflow-cli/tests/p3_8_cross_hop_e2e.rs`). A poisoned mutex from a
  panic in a previous test would propagate as another panic rather than
  a graceful error.
  **Why it matters**: per the project rule "Never use unwrap()/expect()
  except in test code, examples, prototypes". `MockTransport` is shipped
  as a public API surface (one of three `Transport` impls users see) and
  is documented in the rustdoc example block. Either it is a public API,
  in which case the unwraps are forbidden, or it should be moved behind
  `#[cfg(any(test, feature = "test-utils"))]`.
  **Fix**: gate `MockTransport` behind a `test-utils` feature (the type
  is still callable from integration tests across crates) and either keep
  the unwraps (acceptable for a test-only API) or replace with
  `tokio::sync::Mutex` + proper error propagation.

- [M4] `MCPServerHandler::get_server_info` default returns hardcoded
  `version: "0.1.0"` — `src/server.rs:62-68`
  **What**: the default impl returns `{"name":"agentflow-mcp-server",
  "version":"0.1.0"}`. The crate's actual version is 0.2.0 (Cargo.toml:6).
  The client side correctly uses `env!("CARGO_PKG_VERSION")` for
  `Implementation::agentflow()` (`src/protocol/types.rs:248`).
  **Why it matters**: the Beta wire contract pinned by `tests/server_
  contracts.rs` now bakes "0.1.0" into the server-side wire shape. Bumping
  the literal in source breaks the fixture comparison (a "Beta wire
  contract change"); leaving it lies to clients about server version.
  **Fix**: replace the literal with `env!("CARGO_PKG_VERSION")` and
  update the fixture in `tests/fixtures/server_contracts/initialize.json`
  to either match the new value or stop pinning `serverInfo.version`
  exactly (additive-tolerant path already documented in
  `fixtures_tolerate_additive_response_fields`).

- [M5] No request-id collision protection on server side, no oversized-
  payload guard, no method allowlist — `src/server.rs:139-219`
  **What**: `handle_request` accepts any JSON `Value` and trusts that
  `request["id"]` (whatever shape it is, including arrays/objects) maps
  back into the response unchanged. Server side has no message-size cap
  (the client has `max_message_size` at 10 MB default; server has none on
  reads from stdin). The method dispatch is a `match` on string literals,
  not an allowlist — unknown methods get `MethodNotFound`, which is
  spec-correct but the dispatch model offers no hook for capability-
  scoped method filtering.
  **Why it matters**: a malicious or malformed client can send `{"id":
  {"$ref": "huge"}, ...}` and crash the server later if any handler tries
  to serialize the response. The 10 MB cap on the client side does not
  apply when this crate is used as a server.
  **Fix**: add an explicit `id_is_string_or_number` precondition on the
  server, mirror the `max_message_size` knob on the server's stdio loop,
  and expose a `MethodAllowlist` decorator that wraps `MCPServerHandler`.

- [M6] Concurrent in-flight requests are serialized by `Arc<Mutex<Box<
  dyn Transport>>>` — `src/client/session.rs:48, 137, 342`
  **What**: every client call (`list_tools`, `call_tool`, `read_resource`,
  etc.) goes through `transport.lock().await.send_message(...)`. Even
  though multiple tokio tasks may hold the client `Arc`, the mutex
  serializes all access including the read of the response. JSON-RPC
  explicitly allows pipelining (multiple requests in flight, responses
  return in any order), which is the entire reason `id` correlation
  exists in the spec.
  **Why it matters**: the ReAct agent (`agentflow-agents::ReActAgent`)
  H3 batch dispatcher (`run_with_context`) runs idempotent tool calls
  in parallel via `futures::future::join_all`. When those tools are MCP-
  backed, the parallelism collapses to serial at the transport mutex.
  Latency benchmarks in `tests/mcp_latency_benchmarks.rs` measure 50
  sequential `tools/call`s on one reused client — exactly the path this
  mutex makes optimal; the benchmark would not surface the contention.
  **Fix**: rework `MCPClient` around a background reader task plus a
  `DashMap<RequestId, oneshot::Sender<Value>>` so writes and reads are
  decoupled. Same restructure also fixes [C1]. Without [C1] resolved
  this is unsafe to attempt.

### MINOR

- [m1] Unused `anyhow` and `futures` dependencies — `Cargo.toml:25, 33`
  **What**: `anyhow = "1.0"` and `futures = "0.3"` are declared, but
  `grep` finds no usage in `src/`. Bloats the build graph.
  **Fix**: remove both lines; `thiserror` covers the error story and
  `tokio` covers async primitives.

- [m2] Feature flags `client`, `server`, `stdio`, `http` declared but gate
  nothing — `Cargo.toml:42-46`
  **What**: declared in `[features]` but no `#[cfg(feature = "...")]` in
  any source file. Downstream crates cannot opt out of e.g. server-only
  code by toggling features.
  **Fix**: either delete the unused feature flags, or actually gate the
  `server` / `client` modules behind them so a CLI that uses only the
  client side doesn't compile the server.

- [m3] `MCPClient::call_tool` does not auto-validate against the tool's
  schema — `src/client/tools.rs:251-311`
  **What**: `call_tool(name, args)` sends to the server without consulting
  the previously-listed tools' input schemas. The schema-validating
  alternative is `call_tool_validated(&tool, args)` (line 348), which
  requires the caller to have already cached a `Tool` definition.
  **Why it matters**: every caller has to opt in to validation, and the
  CLI / agent paths today use `call_tool` (the unchecked variant), so
  schema mismatches surface as opaque server errors rather than a clean
  client-side validation error.
  **Fix**: add a `validate: bool` field on `ClientConfig` so the client
  can transparently look up the cached tool definition (from a prior
  `list_tools` call) and validate before sending. Or change the default
  call path to validate and add `call_tool_unchecked` as the escape hatch.

- [m4] `tools.rs` public types lack rustdoc — `src/tools.rs:9-65`
  **What**: `ToolDefinition`, `ToolCall`, `ToolResult`, `ToolContent`,
  `ResourceReference`, `ToolRegistry`, `ToolConfiguration` and their
  public methods have no `///` doc comments. ~17 public items.
  **Fix**: this module is mostly older — pre-`client/tools.rs` — and
  could likely be deprecated in favor of `agentflow_mcp::client::Tool` /
  `CallToolResult`. Either document it or mark it `#[deprecated]` and
  point users at `crate::client::*`.

- [m5] Server `handle_request` error path for unparseable params returns
  `?` (uses `From<serde_json::Error>`) — `src/server.rs:182`
  **What**: `serde_json::from_value(params)?` propagates as
  `MCPError::Serialization`, which `run_stdio` then logs and **silently
  continues** (line 116-118). The client never sees a JSON-RPC error
  response, just a hung request.
  **Fix**: wrap the parse in an explicit `match` and return a JSON-RPC
  `InvalidParams (-32602)` envelope to the client.

- [m6] No exponential-backoff jitter in `RetryConfig::backoff_duration` —
  `src/client/retry.rs:40-47`
  **What**: backoff is `base * 2^attempt` capped at `max_backoff_ms`,
  no jitter. Under transient server load with N clients all retrying at
  the same `base_ms`, they retry in lockstep.
  **Fix**: add `with_jitter(JitterStrategy::Decorrelated)` à la AWS SDK.
  Low priority — affects only multi-client deployments.

- [m7] `validate_tool_arguments` recompiles the JSON Schema on every call
  — `src/client/tools.rs:384-389`
  **What**: `JSONSchema::options().compile(input_schema)` runs every time
  `validate_tool_call_arguments` is invoked. For long-running agents
  calling the same tool repeatedly this is wasted CPU on schema parse.
  **Fix**: cache compiled schemas in `MCPClient` keyed by tool name
  (`HashMap<String, Arc<JSONSchema>>`) populated by `list_tools`.

- [m8] `transport_new` deprecation alias is a public re-export with no
  documented removal timeline beyond "future release" — `src/lib.rs:67-76`
  **Fix**: pin a concrete version (e.g. "removed in 0.4.0") in the
  `#[deprecated]` note so external callers can plan the migration.

- [m9] `MCPClient::Drop` is a no-op — `src/client/session.rs:404-409`
  **What**: explicit comment says "Note: Can't use async in Drop, so
  transport cleanup happens in its own Drop". `StdioTransport::Drop` then
  also leaves the actual kill work to `Command::kill_on_drop(true)`. Two
  layers of "we hand off to someone else" — the kill happens via tokio
  internals only if the runtime is still alive. If a panic tears down
  the runtime before the `Child` is dropped, the subprocess leaks.
  **Fix**: low risk in practice — `tokio::process::Command::kill_on_drop`
  is reliable when the runtime outlives the Child. Document the
  assumption explicitly.

- [m10] Latency benchmark has no regression gate — `tests/mcp_latency_
  benchmarks.rs`
  **What**: prints p50/p95/avg to stdout, no assertion against a baseline.
  Run only when invoked explicitly with `--nocapture`. The xtask
  `bench-gate` infrastructure (per recent commit ff77b66) doesn't pick
  this benchmark up.
  **Fix**: emit JSON to a known path under `target/`, plug into the same
  `bench-gate` pipeline used by other crates.

- [m11] `client::session::initialize` does not check returned
  `protocolVersion` against `MCP_PROTOCOL_VERSION` — `src/client/session.rs:
  198-212`
  **What**: deserializes `InitializeResult` (which has a
  `protocol_version: String` field) and stores capabilities, but never
  compares the server's version string to the client's
  `MCP_PROTOCOL_VERSION` (= "2024-11-05"). A server speaking a future or
  incompatible version is accepted silently.
  **Fix**: log a warning when versions disagree (or fail-closed in a
  strict mode). The MCP spec actually requires content-negotiation here:
  if the server doesn't support the client's requested version, the
  client must either continue with the server's version (if it can) or
  disconnect.

### POSITIVE OBSERVATIONS

- W3C `traceparent` injection / extraction (`src/protocol/traceparent.rs`)
  is exemplary: the wire contract is documented at the module level, the
  inject path covers all 4 branches of "params shape" (None / Object /
  Array / Primitive) with explicit comments on why arrays + primitives
  are no-op, and tests cover both directions including "empty string is
  preserved verbatim" and "non-object `_meta` gets overwritten". This is
  exactly the kind of cross-hop carrier code that breaks silently when
  done casually; here it's locked down by 14 unit tests.
- Server-side **Beta wire contract pinning** via JSON fixtures + dotted-
  path assertions (`tests/server_contracts.rs` + `tests/fixtures/
  server_contracts/*.json`) is a strong pattern that several other
  crates would benefit from. The format explicitly tolerates additive
  optional fields, which matches the documented Beta promise.
- Property-based tests (`proptest`) cover the protocol invariants:
  RequestId round-trip, JSON-RPC envelope round-trip, retry-backoff
  exponential growth + cap. ~30+ property cases across the crate.
- `MCPError::is_transient()` cleanly separates retry-eligible errors
  (Transport / Connection / Timeout) from fail-fast (Validation /
  Protocol / Tool). `retry_with_backoff` consults it correctly so non-
  transient errors fail immediately instead of consuming the retry
  budget.
- `Drop` impl in `StdioTransport` has a detailed comment (`src/transport/
  stdio.rs:382-400`) explaining why naive `block_on(process.kill())`
  deadlocks. This kind of "future maintainer save" is rare and valuable.
- Latency benchmark (`tests/mcp_latency_benchmarks.rs`) measures the
  three latency-relevant scenarios (first connect, reused list, reused
  call, full reconnect cycle) and prints p50/p95/avg — methodologically
  sound, just needs the regression gate.

## Metrics

- Source files: 14 (.rs under src/)
- Lines of code: ~4,600 (source incl. inline tests); 8,900 incl.
  integration tests + examples
- Transports: 2 implemented (Stdio, Mock), 2 declared-but-unimplemented
  (Http, HttpWithSSE) in `TransportType`
- Test files: 8 internal `#[cfg(test)]` modules (counts of test functions:
  error 8, builder 7, session 3, tools 9, resources 4, prompts 5, retry
  6, types 9, mock 4, stdio 24, traceparent 14) + 6 integration test
  files (client_integration 11, state_machine_tests 20, timeout_tests 14,
  server_contracts 8, traceparent_propagation 3, mcp_latency_benchmarks 1)
  = ~134 test functions total, plus ~30 proptest cases
- `unwrap()/expect()` in non-test code: **0** in actual src/ production
  paths. All instances are inside `#[cfg(test)]` modules, examples, or
  `tests/` (verified by inspecting all 11 reported lines). The 14
  `Mutex::lock().unwrap()` in `transport/mock.rs` are arguably "non-test
  code" because `MockTransport` is publicly re-exported and used by
  other crates' tests — flagged as [M3], not [C].
- TODO/FIXME/XXX/HACK: **0** in src/ or tests/
- Public items missing rustdoc: ~17 (concentrated in `src/tools.rs`,
  which appears to be a legacy module mostly superseded by
  `src/client/tools.rs`). Elsewhere rustdoc coverage is good.

## Recommendations (prioritized)

1. **Fix the response-id correlation bug [C1] before anything else.** It
   is a latent silent-corruption hazard the moment a real MCP server
   emits notifications. Restructure the transport as background-reader
   + `oneshot` channels keyed by id; this also unblocks [M6] (parallel
   in-flight requests) as a follow-up.
2. **Drain stderr in the stdio transport [C2].** Spawn a `tokio::task`
   that reads-and-discards (or buffers to a bounded ring) the child's
   stderr. This is a guaranteed-eventual hang otherwise.
3. **Plumb subprocess isolation knobs [M1].** Add `env_clear` +
   `current_dir` + env filter to `StdioTransport`, then default the
   `agentflow-skills::mcp_tools` adapter to clean-env. Secrets leakage
   to third-party MCP servers is a real risk in platform mode.
4. **Decide HTTP transport's fate [M2].** Either implement the spec's
   HTTP+SSE transport (the dominant flavor for hosted servers) or
   delete the unimplemented variants from the public `TransportType`
   enum. Half-stated promises rot.
5. **Gate `MockTransport` behind `test-utils` feature [M3].** Or accept
   the unwraps as test-only and document it; either way, stop shipping
   a `pub` panic surface as production API.
6. **Wire `env!("CARGO_PKG_VERSION")` into the server's default
   `get_server_info` [M4]**, and update the contract fixture to either
   match or stop pinning the version literal.
7. **Add the registry adapter to `agentflow-tools` directly, or update
   CLAUDE.md** to point at `agentflow-skills::mcp_tools` as the actual
   location. The doc claim and the code don't agree today.
8. **Cache compiled JSON Schemas in the client [m7]**, add jitter to
   retry backoff [m6], and validate tool args by default with an
   `unchecked` escape hatch [m3]. These are quality-of-life wins.
9. **Plug the latency benchmark into the `bench-gate` xtask [m10]** so
   regressions land in CI instead of being invisible.
10. **Tag the `transport_new` deprecation with a concrete removal
    version [m8]** so downstream consumers can plan.

End of report.
