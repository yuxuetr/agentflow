# Audit: agentflow-worker

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-worker/
**Crate version**: 0.1.0 (workspace v0.2.0+, targeting v0.3.0)
**Layer**: L4 (Operations / Productization)
**Stability tier**: experimental — explicitly marked in module rustdoc for `WorkerResourceLimits` and `WorkerCancellationToken`; `docs/STABILITY.md` flags the distributed worker control plane as experimental until N10 closes.

## Scope summary

`agentflow-worker` is the standalone distributed worker runtime + binary. The crate ships two artifacts: a `WorkerRuntime<P>` library (transport-agnostic loop of heartbeat → claim → execute → report) and a thin `main.rs` binary that wires `WorkerRuntime` to either `InMemoryWorkerProtocol` (`memory://local`) or `GrpcWorkerProtocol` (`grpc://...`). The worker dispatches `NodeExecutionPayload`s on `payload.node_type` across 7 supported node types (`template`, `file`, `mock`, `llm`, `http`, `mcp`, `agent`). Resource enforcement is in-process (timeout + output cap) plus a cooperative `WorkerCancellationToken`.

The crate is intentionally thin: 1156 lines of source (1034 lib + 122 bin) with no per-crate proto or build.rs. The gRPC client is the `GrpcWorkerProtocol` re-exported from `agentflow-server::scheduler::grpc`, and the canonical `WorkerProtocol` trait + `pb` proto module both live in `agentflow-server`. This is a deliberate single-source design but produces some surprising dependency edges (the worker crate imports the server crate).

## Findings

### CRITICAL

- [C1] gRPC worker↔server channel has no transport security (no TLS) and no auth — `agentflow-worker/src/main.rs:27-29`, `agentflow-server/src/scheduler/grpc.rs:484-498`
  **What**: `GrpcWorkerProtocol::connect` calls `Endpoint::from_shared(endpoint).connect()` with zero TLS configuration, no client identity, no bearer-token interceptor, and the binary passes `grpc://host:port` directly through `grpc_endpoint()` which strips the scheme and rebuilds `http://host:port`. The server side (`WorkerControlServer`) is plain h2c via `Server::builder().add_service(...)`. Nothing on the wire prevents an arbitrary host from posing as a worker, claiming tasks (which contain arbitrary JSON payloads handed to `LlmNode`/`HttpNode`/`MCPNode`/`FileNode`), reporting fake results, or DoS-ing the queue.
  **Why it matters**: A worker claim is a privileged operation — claimed tasks may carry filesystem writes (`FileNode`), arbitrary URLs (`HttpNode`), spawn child processes (`MCPNode` via stdio), and execute model calls billed to the operator. The control plane never validates *what* a worker is, only its self-declared `WorkerId`. The `AuthenticatedControlPlane` (`agentflow-server/src/scheduler/admission.rs`) does enforce PSK/JWT admission, but the gRPC adapter wires `WorkerControl` directly onto `WorkerControlPlane<P>` (`agentflow-server/src/scheduler/grpc.rs:270`), **bypassing the authenticated façade**. The doc string at `agentflow-server/src/scheduler/grpc.rs:895`-ish acknowledges this gap ("admission-token metadata propagation lands ... deferred follow-up"). Until admission tokens travel in gRPC metadata, distributed mode is single-tenant-trusted-network only.
  **Fix**: (1) wire the gRPC `WorkerControlServer` through `AuthenticatedControlPlane` and require admission-token gRPC metadata on every call (server: tonic `Interceptor`; client: `Channel::intercept(...)`); (2) add `WorkerConfig::with_tls(...)` taking `tonic::transport::ClientTlsConfig` plus matching `--server-ca`/`--client-cert`/`--client-key` CLI flags; (3) document that the `memory://local` transport is the only auth-exempt path. Until both ship, `docs/DISTRIBUTED.md` should mark the gRPC path as **NOT production-safe** rather than "the target deployment shape."

### MAJOR

- [M1] `WorkerRuntime::run_forever` aborts on the first transport error — `agentflow-worker/src/lib.rs:250-258`
  **What**: The loop is `loop { ...; self.run_once().await?; sleep(...).await; }`. The `?` on `run_once` propagates `WorkerError::Scheduler(SchedulerError::Transport { .. })` out of `run_forever`, which `main.rs:42` turns into a process exit. A momentary control-plane restart, network blip, or transient gRPC `UNAVAILABLE` kills the worker permanently.
  **Why it matters**: Production-grade workers are expected to ride out control-plane restarts (rolling upgrades, deploys, gRPC keepalive timeouts). Today the only recovery is an external supervisor restarting the process; an exponential-backoff in-process reconnect is missing entirely. The `GrpcWorkerProtocol` itself holds an `Arc<Mutex<Grpc<Channel>>>` (`agentflow-server/src/scheduler/grpc.rs:481`), and the underlying `Channel` does support reconnect, but a `tonic::Status::Unavailable` returned mid-call still becomes `SchedulerError::Transport` which `run_forever` fatally bubbles.
  **Fix**: Categorize `WorkerError::Scheduler(Transport)` as recoverable in the loop; log + sleep + retry with bounded exponential backoff. Promote unrecoverable errors (e.g. `InvalidConfig`) to terminal. Add a `--max-reconnect-attempts` flag for ops who want fail-fast.

- [M2] No SIGINT/SIGTERM signal handler — `agentflow-worker/src/main.rs:34-44`
  **What**: `main` calls `runtime.run_forever()` directly with no `tokio::signal::ctrl_c()` or `unix::signal(SignalKind::terminate())` integration. `Drop`-ing the runtime aborts in-flight tasks at the next `.await`. The cancellation token (`WorkerCancellationToken`) exists and is the right shape for graceful drain, but `main.rs` never installs a signal hook that flips it.
  **Why it matters**: Kubernetes / systemd send SIGTERM and wait `terminationGracePeriodSeconds` for the process to drain. Without a handler, an in-flight node execution (which may be writing files via `FileNode`, holding an MCP subprocess, or holding an LLM HTTP connection) is killed mid-flight when the runtime calls `SIGKILL` at the grace deadline. Worse, no `worker.task.cancelled` event is emitted for the abandoned task, so the scheduler waits the stale-heartbeat timeout (default 5s + retries) before requeueing.
  **Fix**: In `main.rs::run`, `tokio::select!` between `runtime.run_forever()` and `tokio::signal::ctrl_c()` (plus a Unix `terminate()` stream). On signal, call `runtime.cancellation_token().cancel()`, then `await` the runtime up to a drain deadline before exiting.

- [M3] gRPC client serializes ALL RPCs through a single `Mutex<Grpc<Channel>>` — `agentflow-server/src/scheduler/grpc.rs:481, 515`
  **What**: Every `submit_task` / `claim_task` / `report_result` / `heartbeat` call takes `self.inner.lock().await` before any wire activity. Heartbeats, claims, and result reports cannot proceed concurrently from the same worker. Today the worker only has `free_slots = 1` so this is invisible, but the moment a worker reports `free_slots > 1` (next milestone), the mutex serializes the heartbeat behind a slow `report_result` and risks stale-heartbeat reaps.
  **Why it matters**: `tonic::transport::Channel` is `Clone + Send + Sync` and is designed for many concurrent in-flight RPCs over one HTTP/2 connection. The mutex defeats that. With H/2 multiplexing this is pure latency tax.
  **Fix**: Drop the `Mutex<Grpc<...>>`. Either store `Arc<Channel>` directly and create a fresh `Grpc::new(channel.clone())` per call (Grpc is cheap to construct), or generate the tonic client with `tonic-build` which produces an internally-clone-friendly client. Add a regression test where 4 concurrent `heartbeat`+`claim` calls all return within ~1× round-trip.

- [M4] `free_slots` is advertised but never enforced; concurrency is hard-pinned to 1 — `agentflow-worker/src/lib.rs:98, 115, 214-247`
  **What**: `WorkerConfig::free_slots` defaults to 1 and is only used at `lib.rs:218` to populate the outgoing heartbeat. `run_once` claims **one** task per call regardless. There is no `tokio::spawn` per task, no semaphore, no parallel dispatcher. A worker advertising `free_slots=4` is lying to the scheduler — capability/locality dispatch (`P10.16.2-FU1`) trusts that number for placement decisions.
  **Why it matters**: The whole point of distributed workers is to amortize one process across many concurrent node executions (especially LLM I/O-bound ones). Today the user has to spin up N separate worker processes, which multiplies memory cost and complicates trace stitching.
  **Fix**: Introduce a `tokio::sync::Semaphore` of size `free_slots`. `run_forever` spawns each `execute_*_payload` on its own `tokio::task` holding a permit. Heartbeat dynamically reports `permits_available` rather than the static config. Add a smoke test with `free_slots=4` and 4 in-flight 1s mock tasks completing in ~1s wall clock.

- [M5] Proto file is **not** the source of truth — `agentflow-server/src/scheduler/grpc.rs:33-152` vs `agentflow-server/proto/agentflow/scheduler/v1/worker.proto`
  **What**: The `pb` module hand-writes prost-annotated structs for `WorkerTask`, `ClaimTaskRequest`, `HeartbeatRequest`, etc. The `.proto` file exists but no `tonic-build`/`prost-build` consumes it. The `.proto` is already out of date: it does **not** declare `accepted_node_types` (tag 2), `locality_run_id` (tag 3) on `ClaimTaskRequest`, the `node_type` field on `WorkerTask`, or `accepted_node_types` (tag 5) on `HeartbeatRequest` — all of which exist in the hand-written `pb` module (P10.16.2-FU1).
  **Why it matters**: Third-party language bindings (Python worker, Go worker, gateway dashboard) that generate code from `worker.proto` will be incompatible with the Rust wire format. The hand-written approach also means no `.pb.rs` snapshot test, no reflection service, and no easy ABI freeze gate.
  **Fix**: Add a `build.rs` to `agentflow-server` (or move proto to a new `agentflow-proto` crate) that runs `tonic_build::compile_protos`. Sync the `.proto` to match the hand-written struct fields, then delete the hand-written module in favor of the generated one. Add a `cargo xtask proto-check` that diffs the generated output against the committed snapshot.

- [M6] `agentflow-worker` depends on `agentflow-server` — layering inversion — `agentflow-worker/Cargo.toml:25`
  **What**: An L4 worker crate imports another L4 server crate for `WorkerProtocol`, `WorkerTask`, `GrpcWorkerProtocol`, `NodeExecutionPayload`. This means anyone who wants only the worker (no Axum / no SQLx / no Postgres migrations) must compile the entire server tree.
  **Why it matters**: (1) cold compile time bloat for distributed deployments; (2) the architectural diagram in `CLAUDE.md` shows worker and server as **siblings**, not worker-on-top-of-server; (3) breaks the "minimal cross-crate dependencies, well-defined public APIs" principle stated in the same doc; (4) any operator who pip-installs a per-language worker SDK will have a separate code path from Rust, masking this coupling.
  **Fix**: Extract `WorkerProtocol`, the four wire DTOs (`WorkerTask` / `WorkerHeartbeat` / `WorkerTaskResult` / `ClaimHints` / `NodeExecutionPayload`), and the `pb` module into a new `agentflow-scheduler-proto` (or `agentflow-worker-proto`) crate that both server and worker depend on. The server keeps `WorkerControlPlane` / `AuthenticatedControlPlane` / `DistributedDagScheduler`; the worker stops importing `agentflow-server` entirely.

### MINOR

- [m1] No per-node-execution memory cap; documented gap is acknowledged but not surfaced in CLI — `agentflow-worker/src/lib.rs:48-79`, `docs/DISTRIBUTED.md:345-358`
  **What**: `WorkerResourceLimits` controls wall-clock timeout and serialized output size, but not RSS. The docs correctly say "rely on the container / cgroup runtime." The CLI `--help` (`main.rs:121`) makes no mention of resource limits at all, so a casual operator has no signal that memory caps are external.
  **Fix**: Add `--memory-mib N` to the binary that hard-errors with "memory caps must be enforced by your container runtime; see docs/DISTRIBUTED.md#worker-resource-limits" so operators discover the gap at first use. Optional v2: on Linux, gate a `cgroupv2`-using `WorkerResourceLimits::memory_max` behind a `cgroup-rs` feature flag.

- [m2] No payload validation before dispatch — `agentflow-worker/src/lib.rs:411-417`
  **What**: The worker treats the control plane as fully trusted: `payload: NodeExecutionPayload` is deserialized from `task.payload` and dispatched directly. There is no allowlist of acceptable `node_type`s (a worker that does not have `mcp` enabled in features still attempts to dispatch `mcp` payloads — it would fail at compile-time today because `mcp` is unconditional in `Cargo.toml:18`, but the design has no admission gate). There is no validation that `payload.node_id` matches `task.node_id`, no input-size cap on `payload.inputs` before allocation, and no `parameters` schema check.
  **Fix**: Add a `WorkerConfig::accepted_node_types: Option<HashSet<String>>` (mirroring `WorkerCapabilities`) and reject unsupported types with `Failed { retryable: false }` before any node-specific code runs. Cap `payload.inputs` total serialized size with an explicit error rather than letting `LlmNode`/`MCPNode` OOM on a 1 GiB input map.

- [m3] `run_forever` always sleeps `poll_interval` after a successful claim — `agentflow-worker/src/lib.rs:251-258`
  **What**: After `run_once` returns `Ok(Some(task))` (i.e. work was found and completed), the loop unconditionally sleeps before claiming again. Under load this caps single-worker throughput at `1 / poll_interval` tasks/sec (default 4 tasks/sec).
  **Fix**: Only sleep when `run_once` returned `Ok(None)` (queue was empty). On a successful claim, loop immediately. Pairs naturally with M4 (parallel dispatch).

- [m4] Event batching is per-task, but heartbeats are per-RPC — `agentflow-worker/src/lib.rs:218-220`, gRPC unary
  **What**: Each `run_once` call issues one heartbeat unary RPC + one claim unary RPC + (on completion) one report unary RPC. With `free_slots=1` and `poll_interval=250ms` that's 12 RPCs/sec per worker even when idle. The proto deliberately chose unary RPCs (no streaming), so multiplexing is wasted on a single HTTP/2 connection but the per-RPC framing overhead is paid every time.
  **Fix**: Make heartbeat coalesced — only send when `last_heartbeat_age > heartbeat_interval` (today the runtime always heartbeats every poll regardless of `heartbeat_interval`, which is dead-code in the runtime). Long-term, consider a bidi `WorkerSession` stream where the server pushes claims and the worker pushes heartbeats + results without per-call setup tax.

- [m5] `heartbeat_interval` is configured but never used — `agentflow-worker/src/lib.rs:100, 117`, `main.rs:19, 103-110`
  **What**: `WorkerConfig::heartbeat_interval` is plumbed through the CLI but `run_once`/`run_forever` always heartbeat once per cycle, controlled by `poll_interval`. Dead field.
  **Fix**: Either honor it (track `last_heartbeat` and skip the RPC when fresh) or remove the knob; documenting a knob that does nothing is worse than not having it.

- [m6] Output truncation marker drops the original outputs entirely — `agentflow-worker/src/lib.rs:352-389`
  **What**: When output exceeds `max_output_bytes`, the entire output map is replaced by `{"truncated":true, "limit_bytes":N, "size_bytes":M}`. Downstream nodes that expected the original outputs receive nothing usable. A bounded *prefix* (first 1024 bytes) inside the marker would preserve some debuggability.
  **Fix**: Add `truncated_prefix: String` to the envelope carrying up to e.g. 1 KiB of pre-truncation content. Pair with a doc note that downstream nodes must check `truncated == true`.

- [m7] Hand-written `Args::parse` instead of `clap` — `agentflow-worker/src/main.rs:67-118`
  **What**: The CLI parser is a custom loop over `std::env::args()` with bespoke error messages and a single hand-written `--help`. The rest of the workspace (`agentflow-cli`) uses `clap`. This is ~50 lines of dead code that drifts from the rest of the project (no `--version`, no subcommand support, no env-var fallback display).
  **Fix**: Use `clap::Parser` with `#[arg(env = "AGENTFLOW_WORKER_ID")]` etc. ~20 lines, free `--help` / `--version` / shell completions / env var documentation.

- [m8] Public items lack rustdoc on the binary's `Args` struct (intentional — it's private) but on the library, `WorkerConfig` fields (`worker_id`, `control_plane`, `free_slots`, `poll_interval`, `heartbeat_interval`) are `pub` without `///` doc — `agentflow-worker/src/lib.rs:95-108`
  **What**: Only `capabilities` and the two `with_*` builders have docs. The struct-level doc says "Worker process configuration." but does not describe what `control_plane: String` should look like (the scheme parser lives in `main.rs`, hidden from library consumers). A downstream consumer integrating `WorkerRuntime` into their own binary has to read `main.rs` to learn the format.
  **Fix**: Document each public field — formats for `control_plane`, units for `poll_interval`, semantics of `free_slots`. Move `grpc_endpoint` parsing into the library so library users get the same scheme handling.

- [m9] `unwrap_or_else(|_| json!({}))` and `unwrap_or_default()` swallow serialization errors silently — `agentflow-worker/src/lib.rs:358, 654`
  **What**: `serde_json::to_value(&outputs).unwrap_or_else(|_| json!({}))` at line 358 silently drops the entire output on a serialization error; the worker would report "success" with an empty body. Similar for `result.answer.clone().unwrap_or_default()` (an agent that returned `None` for `answer` produces a worker output with `answer: ""` indistinguishable from a deliberate empty answer).
  **Fix**: For 358, return `Failed { retryable: false, error: "output serialization failed: ..." }` — a serialization failure indicates a programming bug in a node, not a transport hiccup. For 654, distinguish `None` (`null`) from empty string so the agent stop reason carries the truth.

- [m10] Tests live in two places: `src/lib.rs` mod tests AND `tests/*.rs` integration — `agentflow-worker/src/lib.rs:712-1034`
  **What**: 323 lines of test code at the bottom of `lib.rs` (some duplicating the integration-test patterns in `tests/`). The unit tests reference `agentflow_server::scheduler::distributed::{mock_flow, mock_node}` which is the same import the integration tests use, so the test→implementation distance is identical — there's no value in having them inside `lib.rs`.
  **Fix**: Move the inline `#[cfg(test)] mod tests` into a new `tests/runtime_smoke.rs` or `tests/grpc_roundtrip.rs`. Keeps `lib.rs` under ~700 lines and clarifies the test-file inventory (the metric counts below currently undercount tests because of this split).

### POSITIVE OBSERVATIONS

- Zero `unwrap()`/`expect()` in non-test code. The two `unwrap_or_else` calls noted in [m9] swallow rather than panic. Excellent discipline; matches the global CLAUDE.md no-panic rule.
- Resource-limit + cancellation design is clean. `WorkerCancellationToken` is an `Arc<AtomicBool>` (zero-cost clone), and `tokio::select! { biased; ... cancel ... dispatch ... }` correctly prioritizes cancellation. The `cancelled_during_dispatch` helper produces a well-shaped event stream.
- Worker-local trace events carry consistent `seq` numbering that the control-plane stitching layer relies on (`worker.task.started` → optional `worker.task.output_truncated` → terminal). `cap_success_output` even re-indexes events at line 463 to keep the stream monotonic — easy to get wrong, gotten right.
- The 7-way `execute_supported_node_payload` dispatch table is straightforward `match` on `payload.node_type.as_str()` with a default arm that produces a non-retryable `FlowDefinitionError`. The "typo never hot-loops" invariant is locked in by `unsupported_node_type_returns_structured_failure` (`tests/dispatch_simple.rs:53`).
- Test coverage of the protocol surface is genuinely thorough: 6 documented failure-domain scenarios (`failure_domains.rs`), 4 resource-limit scenarios (`resource_limits.rs`), 3 dispatcher-routing scenarios (`dispatch_simple.rs`), and 2 LLM/agent happy-paths (`dispatch_llm_and_agent.rs`), plus the inline `lib.rs` tests covering gRPC round-trip and the 100-node two-worker smoke.
- W3C `traceparent` propagation is wired through the gRPC call site (`agentflow-server/src/scheduler/grpc.rs:525-538`) and is consumed by both client and server (`inject_traceparent_into_grpc_request` + `extract_traceparent_from_grpc_request`). Distributed spans stitch onto the parent run trace correctly.
- The `mock` dispatcher's `sleep_ms` / `output_size_bytes` / `fail_until_attempt` knobs are an elegant way to keep the timeout / cancellation / truncation / retry tests hermetic without spawning real long-running nodes or wrestling wall-clock races.

## Metrics

- Source files: 2 (`src/lib.rs`, `src/main.rs`)
- Lines of code: 1156 total (lib 1034, bin 122). After excluding inline tests (lib lines 712-1034), production source is ~833 lines.
- Supported node payloads: 7 (`template`, `file`, `mock`, `llm`, `http`, `mcp`, `agent`)
- Test files: inline tests in `src/lib.rs` (~10 tests, 323 lines) + 4 integration files (`dispatch_llm_and_agent.rs`, `dispatch_simple.rs`, `failure_domains.rs`, `resource_limits.rs`) totaling 874 lines and ~16 tests
- `unwrap()/expect()` in non-test code: 0 (the only "unwrap-shaped" calls are `unwrap_or_else`/`unwrap_or_default` — see [m9] for two cases that hide failures). Top non-test sites:
  - `agentflow-worker/src/main.rs:73` — `env::var(...).unwrap_or_else(|_| "worker-local".into())` (acceptable default)
  - `agentflow-worker/src/lib.rs:358` — `serde_json::to_value(&outputs).unwrap_or_else(|_| json!({}))` (silently drops output, [m9])
  - `agentflow-worker/src/lib.rs:577` — `parameters.get("value").cloned().unwrap_or_else(|| json!(payload.node_id))` (acceptable default)
  - `agentflow-worker/src/lib.rs:654` — `result.answer.clone().unwrap_or_default()` (loses None vs empty distinction, [m9])
- TODO/FIXME/HACK markers: 0 raw markers in source. 7 P-tracker comments (P2.8 × 1, P5.5 × 1, P5.6 × 3, P10.16.2-FU1 × 2) that are descriptive (history), not pending work
- Public items missing rustdoc (estimated): ~7 — the 5 plain `pub` fields on `WorkerConfig` ([m8]), `WorkerError` variants are `#[error("...")]`-documented at the thiserror layer but lack `///` summaries, `WorkerCancellationToken::new` has no `///` (defaults to `Default::default()`).

## Recommendations (prioritized)

1. **Ship gRPC auth + TLS (C1).** Until then, distributed mode should be marked unfit for any multi-tenant or untrusted-network deployment. This is the single biggest blocker to claiming the v1.0-rc distributed feature is production-ready. Wire the `WorkerControlServer` through `AuthenticatedControlPlane` and propagate the admission token in gRPC metadata.
2. **Add in-process reconnect + signal-driven graceful drain (M1, M2).** Two surgical changes that together turn the worker into a well-behaved long-lived process under Kubernetes/systemd. M2 in particular is a 20-line addition to `main.rs`.
3. **Unblock real concurrency (M3 + M4).** Drop the mutex around the gRPC channel, add a `Semaphore`-bounded parallel dispatcher, and report dynamic `free_slots`. This is the only way to amortize a worker process across many in-flight LLM-bound tasks, which is the actual production value of distributed mode.
4. **Make `.proto` the source of truth (M5).** Either generate from `worker.proto` via `tonic-build` or delete the stale file. The current state will silently bite the first non-Rust worker binding attempt. Required before promoting the protocol from `experimental` to `stable`.
5. **Extract a `agentflow-scheduler-proto` crate (M6).** Cleans up the L4↔L4 layering inversion and reduces compile-time bloat for worker-only deployments. Also gives external-language workers a clear "what to depend on" answer.
6. **CLI polish + library doc coverage (m7, m8).** Adopt `clap` and document `WorkerConfig` public fields so library consumers don't have to read `main.rs`. ~1-2 hour change.
7. **Tighten serialization-error paths (m9) and unify test layout (m10).** Low effort, improves diagnosability of worker bugs and clarifies the test inventory.

End of report.
