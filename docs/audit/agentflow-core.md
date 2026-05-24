# Audit: agentflow-core

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-core/
**Crate version**: 0.2.0
**Layer**: L1 (Execution Core)
**Stability tier**: stable (per CLAUDE.md — the contract every higher layer depends on)

## Scope summary

Reviewed all 30 source files under `src/` (`lib.rs` + 23 top-level modules + 6
`plugin/` modules), the 11 files in `tests/`, and all 9 `examples/`. Total ≈
13.3 kLOC of production code (counting the orphan `robustness.rs` excluded
below) and ≈ 13 kLOC of tests. Audit focused on the contract documented in
CLAUDE.md (L1): `Flow` orchestrator, `FlowExecutionMode::{Serial,
Concurrent}` scheduler, `AsyncNode`, `GraphNode`, `NodeType::{Standard, Map,
While}`, `FlowValue::{Json, File, Url}`, checkpoint/resume, retry,
concurrency, resource-management, plugin host (`feature = "plugin"`).

## Findings

### CRITICAL (block release / data loss / security)

_None observed._ Production code obeys the workspace's no-`unwrap()` rule
(every `unwrap()` / `expect()` in `src/` is inside `#[cfg(test)]`, a doc-test,
or a comment). Checkpoint persistence uses temp-file + rename (atomic),
`FlowValue` tagged serde round-trips legacy/raw checkpoints with a loud
warning fallback, and the plugin host's JSON-RPC reader/writer correctly
fails-all-pending on stdout EOF / write errors.

### MAJOR (correctness or production risk, fix before next minor release)

- [M1] **Dead / orphaned 1175-line `robustness.rs` not wired into `lib.rs` and
  references removed types** — `agentflow-core/src/robustness.rs:1-438`
  **What**: The file is on disk and `wc -l` sees ~1175 lines, but `lib.rs`
  does not declare `pub mod robustness;`. Its `use` statement
  (`use crate::{AgentFlowError, AsyncNode, Result, SharedState};` at line 3)
  refers to `crate::SharedState`, which does not exist in the current public
  API (see `grep -n SharedState src/*.rs` — only `AgentFlowError::SharedStateError`
  remains). The entire test module starting at line 440 is commented out with
  `/* ... */`. Because the module isn't in `lib.rs`, `cargo build` never
  exercises this code so the breakage isn't surfaced.
  **Why it matters**: Confuses readers ("what does this CircuitBreaker
  implementation actually do?"), gives the false impression that
  CircuitBreaker / Bulkhead exist in this crate (CLAUDE.md never claims them
  for L1; they belong in agentflow-tools / agentflow-nodes if at all), and
  consumes 1175 lines of `wc -l` accounting that distorts crate metrics.
  Also blocks anyone who tries to repair the file from succeeding because
  `SharedState` was removed during the V2 rewrite.
  **Fix**: Either delete the file outright, or move it into a dedicated
  `dead_code/` directory under the workspace root with a README explaining
  it's a v1 archive. Do not leave half-renovated code in `src/`.

- [M2] **`topological_sort` produces non-deterministic order for independent
  nodes** — `agentflow-core/src/flow.rs:1363-1405`
  **What**: `in_degree` and `adj` are built by iterating
  `self.nodes: HashMap<String, GraphNode>` (line 1364-1379). The initial
  queue at line 1381 collects nodes with `in_degree == 0` via
  `in_degree.iter().filter(...)`, which yields HashMap iteration order — i.e.
  non-deterministic between process runs because of `RandomState` hashing.
  As a result, given a workflow with two roots `A` and `B` that both have no
  dependencies, run 1 may execute `[A, B]` and run 2 may execute `[B, A]`.
  **Why it matters**: Breaks reproducibility of trace replay (`agentflow
  trace replay` will see different `NodeStarted` orderings for the same
  workflow), makes test assertions on event order fragile, and produces
  spurious diffs when comparing two runs of the same workflow. The
  concurrent scheduler (`execute_concurrently` at line 754) iterates
  `sorted_nodes.iter().find(...)` to pick the next ready node, so the
  topological order also influences scheduling priority.
  **Fix**: Use a `BTreeMap` for `in_degree` and `adj`, or sort
  `self.nodes.keys()` once at the top of `topological_sort` and walk them in
  sorted order. The queue should also be seeded by sorted insertion order
  (e.g. push roots in `sorted_nodes_keys.iter()` order).

- [M3] **Retry executor swallows the original error, returning only
  `RetryExhausted`** — `agentflow-core/src/retry_executor.rs:74-77, 159-167`
  **What**: When attempts run out, `execute_with_retry` returns
  `Err(AgentFlowError::RetryExhausted { attempts })`. The original error
  type (NetworkError, RateLimit, AuthFailure, ...) is logged via
  `tracing::error!` but never returned to the caller — callers cannot
  distinguish "retried 3× because of NetworkError" from "retried 3× because
  of permission denied". `execute_with_retry_and_context` has the same
  pattern (line 159: `RetryExhausted` overwrites `error` after retries > 0).
  **Why it matters**: Loss of root-cause information. Higher-layer error
  routing (server's error envelope, CLI's exit-code mapping, observability
  / alerting) needs to know whether retries failed for retryable-but-stuck
  reasons (e.g. service down) or for non-recoverable reasons. The current
  shape collapses both into one variant.
  **Fix**: Either (a) extend `RetryExhausted` to carry the boxed last error
  (`Box<AgentFlowError>`), or (b) introduce
  `RetryExhausted { attempts, last_error: Box<AgentFlowError> }`. Option (a)
  is non-breaking for matchers that only destructure `attempts`.

- [M4] **`RetryStrategy::ExponentialBackoff` panics for delays < 4 ms** —
  `agentflow-core/src/retry.rs:200-211`
  **What**: When `delay < 4`, `jitter_range = delay / 4` rounds to `0`. The
  next line, `rand::random::<u64>() % (jitter_range * 2)`, evaluates as
  `rand::random::<u64>() % 0` — integer modulo by zero, which panics in
  release builds. `jitter` defaults to `true`, so any policy with
  `initial_delay_ms <= 3` and a jitter-enabled exponential strategy can
  panic on attempt 0.
  **Why it matters**: Library panic in a hot path (retry loop) reachable
  with valid public configuration. Defaults are safe (`initial_delay_ms =
  100`), but documentation and the builder API let users set 1 ms or 2 ms.
  **Fix**: Guard with `if jitter_range > 0 { ... } else { delay }`. Add a
  unit test covering `initial_delay_ms = 1` with `jitter = true`.

- [M5] **`with_timeout_context` silently drops its context parameters** —
  `agentflow-core/src/timeout.rs:229-250`
  **What**: The function signature accepts `_operation: &str`,
  `_node_id: Option<&str>`, `_workflow_id: Option<&str>`, prefixes them all
  with `_` to suppress unused-variable warnings, and returns a plain
  `TimeoutExceeded { duration_ms }` with no context attached. The comment at
  line 246 (`// Add context if possible (would need ErrorContext support in
  AgentFlowError)`) acknowledges the gap.
  **Why it matters**: The function is part of the public API
  (`pub async fn with_timeout_context`) and lures callers into passing rich
  context that is then thrown away. Worse, downstream layers may have
  written assertions on the operation/node id appearing in error messages.
  Either deliver the contract or remove the parameters.
  **Fix**: Drop the three `_`-prefixed parameters (breaking change for the
  public API, but the function has no real consumers in-tree —
  `grep -r with_timeout_context` shows only its own tests use it), OR
  upgrade `AgentFlowError::TimeoutExceeded` to carry `operation`, `node_id`,
  `workflow_id` and populate them.

- [M6] **`ScopedPermit::Drop` spawns a tokio task — panics if dropped
  outside a runtime** — `agentflow-core/src/concurrency.rs:349-376`
  **What**: `Drop for ScopedPermit` calls `tokio::spawn(async move { ... })`
  to update statistics. If a permit is held by a struct that gets dropped on
  a thread without a current tokio runtime (e.g. during test teardown,
  during a `Drop` chain reached from non-async code, or after a runtime
  shutdown), `tokio::spawn` panics with "there is no reactor running".
  **Why it matters**: A `Permit` is a RAII guard whose entire purpose is to
  be dropped at scope end. Coupling a `Drop` impl to a tokio runtime makes
  the type fragile and surprising. Loss of a stats update is also more
  acceptable than a panic — the current design optimises the wrong axis.
  **Fix**: Update stats synchronously via an `AtomicUsize` (the
  `current_global_active` field is already a number) instead of using an
  async `RwLock`. The per-workflow / per-node-type counts inside a `HashMap`
  are the actual reason a lock was needed; convert those to `DashMap` (an
  external dep) or accept that they need a `parking_lot::Mutex` (fast
  uncontended) — either way, no tokio spawn at drop time.

- [M7] **Duplicate `ErrorContext` types in `error.rs` and `error_context.rs`**
  — `agentflow-core/src/error.rs:27-110` and
  `agentflow-core/src/error_context.rs:14-209`
  **What**: Two distinct `pub struct ErrorContext` definitions exist.
  `error.rs::ErrorContext` is **not** re-exported from `lib.rs` (so it's
  effectively private), but it is referenced by
  `AgentFlowError::with_context` at `error.rs:281`, which returns a
  `ContextualError` wrapping the private type. Meanwhile,
  `error_context.rs::ErrorContext` is what `lib.rs:42` re-exports as
  `pub use error_context::{ErrorContext, ErrorInfo};`. The two have
  incompatible fields (`error.rs` version has `node_id, workflow_id,
  metadata, cause: Option<Box<dyn Error>>`; `error_context.rs` version has
  `run_id, node_name, node_type, error_chain, inputs, execution_history,
  retry_attempt, metadata`).
  **Why it matters**: Two structs with the same name doing nearly the same
  job is confusing for new contributors and risks divergence over time.
  `AgentFlowError::with_context` produces a value (`ContextualError`) that
  no caller in this crate consumes, but it's a `pub` API so external
  callers may depend on the shape.
  **Fix**: Pick one. The `error_context.rs::ErrorContext` is richer and
  what the rest of the workspace uses (lib.rs re-exports it). Delete
  `error.rs::ErrorContext` (lines 25-110), `ContextualError`, and
  `AgentFlowError::with_context`. Mark the removal in a changelog entry.

### MINOR (code health, docs, style)

- [m1] **Heavy `println!` / `eprintln!` use in production code paths** —
  `agentflow-core/src/flow.rs` (lines 188, 377, 379, 403, 426, 512, 517,
  558, 606, 705, 792, 833, 892, 935, 1029-1083, 1340, 1453) and others
  **What**: ~20+ `println!` calls in the executor with emoji prefixes
  ("▶️ Executing node", "⏭️ Skipping node", "💾 Checkpoint saved",
  "🔍 Evaluating condition", etc.). The crate has an `observability` feature
  that gates a `tracing` dependency, but these `println!`s bypass it.
  **Why it matters**: Spams stdout when consumed as a library; produces
  non-deterministic interleaved output under concurrent execution; cannot
  be silenced or routed by embedders (CLI, server, worker). The `eprintln!`
  warnings for checkpoint-save failures (line 512, 558, 892, 935) are
  visible but also unstructured.
  **Fix**: Behind `#[cfg(feature = "observability")]`, replace with
  `tracing::info!` / `tracing::warn!`. Embedders can then route through
  `tracing-subscriber`. Pure consumers without the feature get silence,
  which is the expected library behaviour.

- [m2] **`Cargo.toml` carries unused deps: `regex`, `url`, `anyhow`** —
  `agentflow-core/Cargo.toml:33, 52, 55`
  **What**: `grep -r 'regex::\|url::\|::Url::' src/` returns nothing.
  `anyhow` appears only in `examples/logging_demo.rs` and a doc comment in
  `checkpoint.rs:20`. None of these crates are actually used by the
  library.
  **Why it matters**: Adds compile time, supply-chain surface, and
  resolver complexity for no benefit. `anyhow` should at minimum move to
  `[dev-dependencies]` so examples still build.
  **Fix**: Remove `regex` (1.0) and `url` (2.5) entirely. Move `anyhow` to
  `[dev-dependencies]`.

- [m3] **`println!("🔍 Evaluating condition: ...")` issues a debug line on
  every `run_if` evaluation** — `agentflow-core/src/flow.rs:1340`
  **What**: Each conditional branch logs a line. Workflows with many
  `run_if`-guarded nodes (a common pattern for skill-driven agents) spam
  stdout.
  **Why it matters**: Same as m1 but called out separately because this
  one fires per-evaluation, not per-execution.
  **Fix**: Same — gate behind `tracing::debug!`.

- [m4] **Bench artifact `[[bench]] name = "hot_paths"` declared but file
  not enumerated by my read** — `agentflow-core/Cargo.toml:97-99`
  **What**: I verified `benches/` exists but did not enumerate its
  contents. Cargo.toml declares two benches (`scheduler`, `hot_paths`); if
  `benches/hot_paths.rs` exists it should be referenced; if not, this is a
  broken Cargo target. (Verify with `cargo bench --no-run --bench
  hot_paths`.)
  **Fix**: Either confirm the file is present or remove the stale
  declaration.

- [m5] **Public scheduler types have zero rustdoc** —
  `agentflow-core/src/scheduler.rs:9-13, 15-23`
  **What**: `FlowExecutionMode`, `FlowExecutionConfig`, and most of their
  builder methods (`serial`, `concurrent`, `with_run_base_dir`,
  `with_cancellation_token`) have no `///` comments. The struct fields are
  also bare. `FlowCancellationToken` is doc'd at line 65 but its `new`,
  `cancel`, `is_cancelled` are not.
  **Why it matters**: This is the public scheduler API — sister crates
  (cli, server, harness) embed it and copy/paste-driven docs would
  benefit. CI gate "rustdoc -D warnings" passes only because lib-level
  warnings don't enforce per-item.
  **Fix**: Add a one-sentence `///` to each public item. Trivial change,
  high readability ROI.

- [m6] **`std::env::set_var("HOME", ...)` in tests races parallel test
  binaries** — `agentflow-core/src/flow.rs:1492-1500`,
  `agentflow-core/tests/checkpoint_recovery_test.rs:24-35`,
  `agentflow-core/tests/state_size_observer_tests.rs:55-62`
  **What**: All three sites mutate the process-wide `HOME` env var via an
  `unsafe { ... }` block, with comments asserting "no other thread
  concurrently mutates the process environment". This is true *within* one
  test binary executing serially, but Cargo's test harness runs tests
  concurrently by default — and these three test files run inside three
  separate test binaries, each of which may parallelise its own tests. The
  comment overstates the safety property.
  **Why it matters**: A flaky `HOME`-dependent test (the CI history at
  60b3987 "test(server): drop global TRUNCATEs that raced parallel test
  binaries" shows the project has hit similar issues before) is hard to
  debug. The same flake risk exists here.
  **Fix**: Plumb a base-dir override through `CheckpointConfig` /
  `FlowExecutionConfig` (already supports `run_base_dir`) and call it
  explicitly in tests — no env mutation needed.

- [m7] **Heavy `Clone` in concurrent dispatcher hot path** —
  `agentflow-core/src/flow.rs:778-832`
  **What**: `execute_concurrently` clones the entire `GraphNode` at line
  784 (`graph_node = self.nodes.get(&node_id).ok_or_else(...)?.clone()`),
  then clones `graph_node.initial_inputs` at 830 and `initial_inputs` at
  831 for every node dispatch. `GraphNode` contains
  `Vec<GraphNode>` templates (Map / While nodes), so the clone is deep and
  non-trivial.
  **Why it matters**: For large DAGs (the `large_dag_benchmarks.rs` target
  exists) this matters. Hot-path Rust style (per workspace's "performance
  optimization" guidelines) prefers `Arc<GraphNode>` so the spawned future
  borrows a cheap-to-clone handle.
  **Fix**: Store `Flow::nodes` as `HashMap<String, Arc<GraphNode>>`. Each
  dispatch clones the `Arc`, not the contents.

- [m8] **`FlowExecutionConfig::Default::default()` collapses to Serial-mode
  but `with_run_base_dir` / `with_cancellation_token` are post-construction
  setters** — `agentflow-core/src/scheduler.rs:41-50, 52-63`
  **What**: Stylistic — the builder is a method-chaining pattern on the
  config itself rather than a dedicated `Builder` struct. That's fine, but
  the lack of doc means callers must read the source to discover
  `serial()` vs `concurrent()` constructors.
  **Fix**: Combine with m5 (rustdoc) — one example block on the type would
  cover it.

- [m9] **`outputs_to_json` swallows serde errors with
  `unwrap_or(serde_json::Value::Null)` / `unwrap_or_else(json!({}))`** —
  `agentflow-core/src/flow.rs:635-644`
  **What**: When serializing a `FlowValue` map fails (a `FlowValue::Json`
  containing e.g. an inf/NaN float), the result silently becomes `Null` or
  `{}`. The downstream checkpoint then stores garbage, and a subsequent
  resume cannot reconstruct the original state.
  **Why it matters**: Same family of silent-data-loss as the
  "tagged-but-corrupt → Json fallback" path in `decode_checkpoint_flow_value`,
  but that one warns loudly (line 705-714) whereas this one is silent.
  **Fix**: At minimum, `eprintln!` (consistent with the surrounding code)
  on serde failure. Better: surface as `AgentFlowError::SerializationError`
  through `persist_step_result`'s `Result` return.

- [m10] **`StateMonitor::record_allocation` rollback race after fetch_add**
  — `agentflow-core/src/state_monitor.rs:254-281`
  **What**: When `auto_cleanup = false` and the new total exceeds
  `max_state_size`, the code does
  `fetch_add(size_delta, ...)` → check limit → `fetch_sub(size_delta, ...)`.
  Between the add and the sub, another thread can do its own add and see a
  transient false "exceeded" alarm — or worse, succeed when it shouldn't
  because two concurrent allocations sum below `max + 2*size` after the
  loser rolls back.
  **Why it matters**: The intent is admission control; the implementation
  is best-effort. Low severity because `auto_cleanup = true` is the
  default.
  **Fix**: Replace with a CAS loop on `current_size`. Or wrap admission
  control in a small `Mutex<usize>` — the path is not on the absolute hot
  path of node execution.

### POSITIVE OBSERVATIONS

- Zero `unwrap()` / `expect()` in non-test production code. All 204
  occurrences are either inside `#[cfg(test)]`, inside doc-comments
  (`//!     ...`), or inside `proptest! { ... }` blocks. The crate truly
  honours the workspace's no-unwrap rule.
- `FlowValue` serde uses a stable tagged schema with a property-based
  round-trip test (`flow_value_json_roundtrip_preserves_variant`) and an
  explicit "legacy raw JSON" decode test. The checkpoint compatibility
  matrix is well covered in `tests/flow_value_checkpoint_compat.rs`.
- Layer boundary clean: `Cargo.toml` has zero `agentflow-*` workspace
  dependencies, so L1 truly sits at the bottom of the graph (confirmed by
  `grep -n agentflow- Cargo.toml`).
- Plugin host (`plugin/host.rs`) correctly fails-all-pending on stdout EOF
  / write errors via `PendingTable::fail_all`, has bounded outbound queue
  depth (64), forwards stderr per plugin name, and uses `kill_on_drop` for
  child cleanup.
- `Checkpoint` save is atomic (temp file + `sync_all` + rename + copy to
  `_latest.json`) — survives mid-write process crashes.
- `decode_checkpoint_flow_value` produces a loud `eprintln!` warning when
  a tagged-but-corrupt value falls back, rather than silent data loss.

## Metrics

- Source files (.rs in src/): 30 (24 top-level + 6 plugin/ + 1 bin/)
- Lines of code (approx, `wc -l` total of src/): **13,259** (excluding
  the orphaned `robustness.rs` which adds 1,175 more)
- Test files: 19 unit (`#[cfg(test)] mod tests` inside src/) + 11
  integration (`tests/*.rs`)
- `unwrap()/expect()` in non-test code: **0** confirmed — all 204 grep
  matches are inside `#[cfg(test)]` regions, doc comments (`//!`), or
  proptest blocks
- TODO/FIXME/XXX in code: **0** (clean)
- Public items missing rustdoc: estimated ~15 public items in
  `scheduler.rs` alone, ~6 in `flow.rs::Flow` setters, ~10 in
  `concurrency.rs` builder. Spot-checked, not exhaustive.

## Recommendations (prioritized)

1. **Delete or relocate `robustness.rs`** (M1). 1175 LOC of broken code
   in `src/` is the single worst signal in the crate.
2. **Make `topological_sort` deterministic** (M2). Reproducibility unlocks
   trustworthy trace replay and stable test assertions.
3. **Preserve original error through retry exhaustion** (M3). Loss of
   root-cause classification is the single biggest observability gap in
   the retry layer.
4. **Fix the `RetryStrategy` panic for `delay < 4` with jitter** (M4).
   Library panic on a public API is a hard rule violation.
5. **Drop the dead `_operation/_node_id/_workflow_id` parameters from
   `with_timeout_context` (M5)** or wire them through to the error.
6. **Replace `ScopedPermit::Drop`'s `tokio::spawn` with synchronous
   atomics** (M6). RAII guards must not require an async runtime to drop.
7. **Deduplicate `ErrorContext`** (M7). Pick the rich one in
   `error_context.rs`; delete `error.rs::ErrorContext` and
   `with_context`.
8. **Route stdout chatter through `tracing` behind `observability`**
   (m1, m3). Today the crate is loud by default.
9. **Trim `Cargo.toml` deps** (m2): remove `regex` + `url`, move
   `anyhow` to dev-dependencies.
10. **Plumb `run_base_dir` everywhere and stop mutating `HOME` in
    tests** (m6). Prevents the next "tests race when run in parallel"
    incident.

End of report.
