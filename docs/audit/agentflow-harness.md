# Audit: agentflow-harness

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-harness/
**Crate version**: 0.1.0
**Layer**: L3 (Agent / Orchestration)
**Stability tier**: beta (per docs/STABILITY.md, promoted at P-H.5 slice 2)

## Scope summary

The harness crate wraps an inner `agentflow_agents::AgentRuntime` (typically `ReActAgent`)
into a long-lived, workspace-aware session with frozen `HarnessEvent` wire envelope,
pluggable `ContextProvider` / `PreToolHook` / `PostToolHook` / `ApprovalProvider`,
JSONL/in-memory/stdout sinks, and an in-process `TaskRuntime` for background child
agents. Phases H0-H4 are reported "closed" in CLAUDE.md. This audit found one
**critical** invariant violation in the event-sequence contract (which is the
load-bearing promise of the Beta-tier envelope), one **critical** silent payload
leak (no redaction), and several MAJOR contract drift issues that block the
crate's claim of a faithful Beta surface.

## Findings

### CRITICAL

- [C1] `seq` namespace split between `HarnessRuntime` and `HookConfig` — runtime/sink seq numbers WILL collide — `agentflow-harness/src/runtime.rs:259, 302-317` vs `agentflow-harness/src/hooks_runtime.rs:84, 529`
  **What**: `HarnessRuntime::run` translates inner-agent events using a **local** `let mut seq = self.initial_seq` (runtime.rs:259) that increments inside `translate_inner_events`. `HookedTool::emit_event` (hooks_runtime.rs:529) uses an unrelated `Arc<AtomicU64>` owned by `HookConfig`. There is no API on `HarnessRuntime` to share or read its seq cursor with `HookConfig`. The server wires this up by passing `seq_counter = AtomicU64::new(0)` to `HookConfig` and a separate `with_initial_seq(inputs.initial_seq)` to `HarnessRuntime` (`agentflow-server/src/harness_live.rs:333,342,357`). At runtime, both counters mint seq 0, 1, 2, … independently, so `approval_requested` from a hook reuses seq numbers already (or later) emitted by the runtime's `session_started` / `step_started` / `stopped` translation.
  **Why it matters**: The frozen envelope promise is "`seq` is monotonically increasing per session, starts at `0`, and must never gap" (event.rs:36-38, HARNESS_MODE.md:75). Trace replay, SSE backfill (`after_seq=N`), and `(session_id, seq)` DB primary keys all rely on this. Collisions break replay determinism; duplicates either get dropped by the DB (silent event loss) or cause backfill to skip past real events. This is a wire-shape correctness bug, not just an implementation nit.
  **Fix**: Promote the `seq` cursor to a single source of truth — store `Arc<AtomicU64>` on `HarnessRuntime` and expose `runtime.seq_counter()` so `HookConfig::with_seq_counter` can wire to the same atomic. Update the H1 emission sites in `runtime.rs:275, 311, 419, 432, 451` to `fetch_add(1, SeqCst)` on the shared counter. Add an integration test that asserts no two events in a session share a seq even when a hook fires between step events.

- [C2] Approval/tool payloads embed raw tool params with no redaction — secrets leak to sinks — `agentflow-harness/src/hooks_runtime.rs:346, 470` and `agentflow-harness/src/runtime.rs:477`
  **What**: `HookedTool::build_pending` stores `params: params.clone()` (hooks_runtime.rs:346); that struct is then handed to `PreToolHook::before_tool` and embedded verbatim into `ApprovalRequest::params_summary` (hooks_runtime.rs:470). Similarly, `tool_call_requested_from_step` (runtime.rs:477) copies the agent's raw tool params into the envelope. Sink dispatch (`JsonlEventSink` / `StdoutEventSink` / DB sink in agentflow-server) writes the unmodified JSON. `agentflow-tracing` ships a `redaction` module but this crate does not import or invoke it.
  **Why it matters**: A tool call like `{"api_key": "sk-…", "url": "…"}` lands in `~/.agentflow/traces/harness/sessions/<id>.jsonl`, in stdout when `--output stream-json` is used, and over SSE to any subscriber. The contract docs explicitly say "Implementations MUST avoid embedding secrets or raw file contents here" (approval.rs:117) and "the runtime MUST redact secrets before constructing this struct" (hooks.rs:39), but no caller is given a hook to enforce this. For a Beta-tier wire surface that is also approved-for-production via `HarnessProfile::Production`, this is a confidentiality regression vs. the rest of the AgentFlow trace pipeline.
  **Fix**: Add a `ParamRedactor` trait (or reuse `agentflow_tracing::redaction::Redactor`) and require it on `HookConfig` + `HarnessRuntime`; default to the tracing crate's standard redactor that strips known-secret keys (`api_key`, `password`, `authorization`, Bearer tokens, env-var values). Apply at the seam where `PendingToolCall::params` and `ToolCallRequestedPayload::params_summary` are built. Document the redaction guarantee in the rustdoc that currently only says "MUST".

### MAJOR

- [M1] `HookedTool::build_pending` hard-codes `step_index: 0` — every approval/hook payload reports the wrong step — `agentflow-harness/src/hooks_runtime.rs:341`
  **What**: `PendingToolCall.step_index` is set to `0` literally; the resulting `ApprovalRequest` inherits it (hooks_runtime.rs:465). There is no plumbing from the inner agent loop to the wrapped tool execution, so even on call 30 of a 50-step ReAct trajectory, `approval_requested` claims `step_index: 0`. The `tool_call_requested` envelope emitted by `HarnessRuntime` post-hoc uses the real index (runtime.rs:471), so two related events for the same call disagree on `step_index`.
  **Why it matters**: `step_index` is a frozen field on the Beta-tier wire surface. UI clients and replay tools correlate approval prompts to the trajectory by step. Reporting 0 for every approval breaks ordering, makes audit logs misleading, and undermines the "deterministic for replay" property.
  **Fix**: Thread a per-call step index from the agent runtime into `Tool::execute`. Options: (a) add `task_local!` step counter that the runtime sets before each call, (b) attach the step index to `params` under a reserved key and strip in `HookedTool`, (c) wrap params in a request envelope at the L3 boundary. (a) is the lowest-touch and parallels how `IN_BACKGROUND_TASK` is already done in tasks.rs:51-55.

- [M2] `stop_after_deny` short-circuit emits no `approval_decided` or `approval_requested` event — silent denials — `agentflow-harness/src/hooks_runtime.rs:441-448`
  **What**: Once any decision in a session is `DenyAndStop`, subsequent calls return `Proceed::Deny` without emitting any approval event (the cache lookup happens before `emit_event` is reached). The test `deny_and_stop_blocks_subsequent_calls_without_reprompt` (hooks_runtime.rs:944) confirms only one `ApprovalRequested` is ever seen.
  **Why it matters**: Operators / UI clients reading the SSE stream observe the agent making tool calls that produce `tool_call_completed { is_error: true }` with no preceding `approval_*` event explaining why. Audit trail loses the cause. Trace replay can't reconstruct the decision history.
  **Fix**: After the early return at line 444-447, synthesise a `cached`-style `ApprovalDecided` event with `decided_by: "cache:stop_after_deny"` so downstream consumers see the gate.

- [M3] Cached `ApprovalDecided` envelopes carry synthetic `request_id` that joins no `ApprovalRequested` — `agentflow-harness/src/hooks_runtime.rs:510-511`
  **What**: `emit_cached_decision` builds `request_id: format!("cached-{tool}")`. The wire contract documents `id` as "the join key between `ApprovalRequest` and the corresponding `ApprovalDecision`" (approval.rs:96-100). For cached re-uses, that synthetic id matches no prior `ApprovalRequested` event in the stream.
  **Why it matters**: UI / replay tools that build a request→decision graph end up with orphan decisions. A reasonable consumer that JOINs on `request_id` to display "User decided X for request Y at time T" cannot do so.
  **Fix**: Cache the original `request_id` alongside the outcome in `ApprovalCache::cached` (key it on `(tool, scope) → (request_id, outcome)`) and re-use the original id in `emit_cached_decision`. Or emit a fresh `request_id` paired with a fresh `ApprovalRequested` event so the wire shape stays joinable.

- [M4] No `tracing_bridge` integration of harness events into `agentflow-tracing`'s `ExecutionTrace` — CLAUDE.md says "Harness sessions … persist to Postgres + SSE only; file-backed trace integration would need a separate adapter and is not wired today"
  **What**: `tracing_bridge.rs` only resolves a directory path (`AGENTFLOW_TRACE_DIR`) and returns a `JsonlEventSink` — it does NOT bridge into `agentflow_tracing::EventListener` / `ExecutionTrace`. So `agentflow trace replay` and the TUI cannot read harness sessions through their normal storage backend, only by hand-parsing the JSONL files.
  **Why it matters**: CLAUDE.md describes a unified observability stack; an L4 productization gap that, given the Beta promotion, callers may not realise. The current rustdoc says the "deeper integration … lands with the server work in Phase H5" (tracing_bridge.rs:5-7), but Phase H5 is reported closed and this seam still hasn't been built.
  **Fix**: Either implement the `HarnessEventListener → ExecutionTrace` adapter or update the rustdoc and CLAUDE.md to reflect that file-trace integration remains a known gap post-H5. Track explicitly.

- [M5] `SinkChain::dispatch` records "first error" but message claims "keeps the remaining writes going" — partial success is invisible to caller — `agentflow-harness/src/persistence.rs:262-276`
  **What**: When sink A succeeds and sink B fails, the caller sees `Err(B_error)` and has no way to know A persisted the event. The runtime's `dispatch(&started_event).await?` (runtime.rs:280) propagates the error and aborts the whole `run()` even though one sink succeeded.
  **Why it matters**: A flaky DB sink alongside a JSONL sink will fail the entire session even though the JSONL was written. The "keeps the remaining writes going" promise is implemented, but the caller-facing contract is single-error which forces fail-stop semantics.
  **Fix**: Either return `Result<DispatchReport, …>` where `DispatchReport` enumerates per-sink outcomes, or change `HarnessRuntime::run` to log dispatch errors and continue (the `tracing::warn!` already fires inside `dispatch`).

- [M6] `Cargo.toml` `tokio` features missing `time` despite heavy use of `tokio::time::{timeout, sleep}` — relies on transitive features — `agentflow-harness/Cargo.toml:27`
  **What**: Declares `tokio = { features = ["fs", "io-util", "sync", "macros", "rt"] }` but `hooks_runtime.rs:358, 384` call `tokio::time::timeout`, `approval_providers.rs:247` calls `tokio::time::sleep`, and `tasks.rs:932, 1013` use `tokio::time::sleep`. Compilation today works only because `agentflow-agents` / `agentflow-tools` happen to pull `tokio = ["full"]` or `time` transitively.
  **Why it matters**: Brittle build — any downstream that pares its tokio features can break compilation of `agentflow-harness`. Workspace builds the crate today because of transitive deps; that's not a contract.
  **Fix**: Add `"time"` to the explicit `tokio` features list.

- [M7] `HarnessProfile::Local` / `Dev` "silent allow" default is documented but production-trap-shaped — `agentflow-harness/src/context.rs:70-82` and `hooks_runtime.rs:97-118`
  **What**: With no pre-hook registered AND no `with_profile(Production)`, all tools — including `shell`, `file:write`, `task_create` — are silently auto-allowed. The HookConfig docs warn at length (98-118) but the default profile (`HarnessProfile::default()` → `Local`) is what `HookConfig::new` picks.
  **Why it matters**: A binary that copies the CLI examples and forgets `.with_profile(Production)` ships with no approval gate on mutating tools. The doc-only warning is not enough for a Beta wire surface intended for productionization.
  **Fix**: Flip the default in `HookConfig::new` to `Production`, and require an explicit `.with_profile(Local).with_explicit_dev_intent(true)` (or similar typed acknowledgement) for the permissive mode. Update the rustdoc + the reference binary in `examples/applications/code-reviewer-write/`.

### MINOR

- [m1] `persistence.rs:142` `.expect("file opened by ensure_open above")` is the only non-test `expect()` — replaceable with `unreachable!()` with explanation per project Rust style guide; current message is fine but the global rule prefers `unreachable!()` with structured rationale.

- [m2] `runtime.rs:298-300` wraps inner-agent failures as `HarnessError::Other(format!("inner agent failed: {err}"))` — loses the typed `AgentRuntimeError` variant. Consider adding a `HarnessError::InnerAgent(AgentRuntimeError)` variant with `#[from]` so callers can still pattern-match.

- [m3] `tracing_bridge.rs:97-106, 112-121` use `unsafe { std::env::set_var }` in tests with a comment claiming serialization "unless `#[parallel]` is used". Tokio multi-threaded tests in the same binary CAN race here. The test module name claims uniqueness but a second test (`env_var_wins_over_default`) overwrites the same var without a lock. Risk: flaky CI. Fix with a `static MUTEX: std::sync::Mutex<()>` like `runtime_react_smoke.rs` already does.

- [m4] `Cargo.toml` has no `[features]` table — every dependency is mandatory. `agentflow-llm` and `agentflow-memory` are `dev-dependencies` only (good), but the smoke test brings them in unconditionally, blocking downstream callers from compiling tests with a lean dependency tree.

- [m5] `HookedTool::execute` (hooks_runtime.rs:287-323) computes `output_summary: None` for the post-hook unconditionally; a `PostToolHook` cannot observe the actual tool output. The struct field exists in `CompletedToolCall`, so the hook is being lied to.

- [m6] `runtime.rs:478` and `runtime.rs:144-145` — `HarnessRunOptions` exposes every field as `pub` (no setters required) but provides chained `with_*` setters too. Mixing makes the API surface ambiguous; prefer one style (typed builder) or document explicitly.

- [m7] `TaskHandle`, `TaskSpec`, `TaskOutputSnapshot`, `HarnessRunOptions`, `HarnessRunResult` fields are `pub` with no `///` doc comments (tasks.rs:96-118, 123-128; runtime.rs:37-54, 120-136). For a Beta wire surface, fields should be individually documented.

- [m8] `TaskRuntime::create_task` emits `TaskStatus::Pending` (tasks.rs:288) but `drive_task` immediately transitions to `Running` in the spawned task (line 376) — there's a race between `create_task` returning and the `Pending` event landing in the sink ordering vs the `Running` event. SinkChain dispatches in order, but the `Pending → Running` events can interleave with other events from the parent loop because the spawned task uses the same `seq_counter`.

- [m9] `TaskWriter::push_line` (tasks.rs:174-189) silently drops a line when it would overflow but does NOT account for newline overhead or the displayed cost of "truncated marker"; truncation accounting is byte-len of `line` only.

- [m10] `cli_provider_honours_expires_at_deadline` test (approval_providers.rs:398-420) accepts BOTH `ApprovalTimeout` AND `Allow` as valid outcomes (lines 415-419). Test is non-deterministic by design; should pin the input to truly block (a `Pending`-state reader) so the deadline path is exercised exactly.

- [m11] `tasks.rs:1080-1104` test creates a task, lets it complete, THEN calls `writer.push_line` against a terminal task. Production usage drives the writer DURING task execution. The test exercises overflow accounting but does not exercise the actual streaming-during-run path.

- [m12] `HookConfig::with_pre_hook` / `with_post_hook` / `with_hook_timeout` / `with_approval_timeout` / `with_seq_counter` (hooks_runtime.rs:155-178) — no `///` docs on these public setters.

### POSITIVE OBSERVATIONS

- Frozen-fixture envelope tests (`tests/envelope_contract.rs` + 4 `.json` fixtures) correctly nail down the wire shape including the "additive field tolerance" promise. Excellent.
- `IN_BACKGROUND_TASK` task-local for nested task rejection (tasks.rs:51-55, 244-248) is the right pattern — clean, automatic, race-free.
- `HookedTool::run_pre_hooks` (hooks_runtime.rs:351-378) is correctly fail-closed on both timeout AND error — both branches return `HarnessError::hook(...)` which maps to `ToolError::PolicyDenied`. Tests cover both paths (`slow_pre_hook_times_out_and_denies`).
- `merge_pre_decisions` (hooks_runtime.rs:552-562) implements the "strictest wins" composition correctly (Deny > RequireApproval > Allow), and the production-profile auto-escalation for `NonIdempotent` calls (lines 416-417) is the right default for fail-closed.
- `HarnessRuntime::with_initial_seq` (runtime.rs:211-214) doc comment is exemplary: explains the "why" (server append-mode resume), the "how" (`MAX(existing seq) + 1`), and the design rationale all in one place.
- Contract types (`HarnessEvent`, `ApprovalRequest`, `ApprovalDecision`, `HarnessContext`, `HarnessProfile`, `HarnessRuntimeKind`) are well-documented with rustdoc that explains stability tier, frozen kind set, and replay semantics.
- L3 dependency boundary is clean: only `agentflow-agents`, `agentflow-tools`, `async-trait`, `serde`, `tokio`, `tracing`, `chrono`, `uuid`, `thiserror`. No `agentflow-llm` / `agentflow-memory` in non-test code.
- Closed envelope kind set in `HarnessEventBody` (event.rs:66-90) matches CLAUDE.md and HARNESS_MODE.md exactly — 9 kinds, no drift.
- 4 default context providers (`AgentsMdProvider`, `TodosMdProvider`, `RoadmapMdProvider`, `WorkspaceLayoutProvider`) all gracefully handle missing files (return empty rather than error). All cap output. `WorkspaceLayoutProvider` excludes dotfiles.
- `tracing_bridge::AGENTFLOW_TRACE_DIR_ENV` precedence (override → env → `~/.agentflow/traces`) matches the workspace-wide convention.
- Test density is high: 13 source files / 5,976 LOC / 47 inline tests + 2 integration test binaries + 4 frozen fixtures.

## Metrics

- Source files: 13
- Lines of code: 5,976 (incl. tests; ~3,400 production)
- HarnessEvent kinds: 9 (`session_started`, `step_started`, `tool_call_requested`, `approval_requested`, `approval_decided`, `tool_call_completed`, `background_task_updated`, `memory_summary_added`, `stopped`) — matches CLAUDE.md frozen set
- Context providers: 4 (`AgentsMdProvider`, `TodosMdProvider`, `RoadmapMdProvider`, `WorkspaceLayoutProvider`)
- Sinks: 3 (`JsonlEventSink`, `StdoutEventSink`, `InMemoryEventSink`) + `SinkChain` aggregator
- Built-in background-task tools: 5 (`task_create`, `task_get`, `task_list`, `task_stop`, `task_output`)
- Approval providers: 3 (`AutoAllowApprovalProvider`, `AutoDenyApprovalProvider`, `CliApprovalProvider`)
- Test files: 47 inline `#[cfg(test)]` test fns across 11 modules + 2 integration test binaries (`envelope_contract.rs`, `runtime_react_smoke.rs`) + 4 JSON fixtures
- `unwrap()/expect()` in non-test code: **1 real `expect()`** (persistence.rs:142, an enforced invariant); 7 `unwrap_or*` calls (all safe fallbacks)
- TODO/FIXME/XXX/HACK: 0 source-level (only doc strings reference "TODOs.md" the file)
- Public items missing rustdoc: ~20 (chiefly `HarnessRunOptions` / `TaskSpec` / `TaskHandle` fields; minor `with_*` setters on `HookConfig`)

## Recommendations (prioritized)

1. **[C1] Unify the seq counter** — the wire-shape promise depends on it. Without this fix, the Beta-tier envelope claim is not honoured under any session that touches both an approval flow and runtime translation (which is every non-trivial session). Block any further H-line work until this is repaired and proven with a regression test.

2. **[C2] Wire `agentflow_tracing::redaction::Redactor` (or equivalent) through `HookConfig` and `HarnessRuntime`** before the next Beta consumer adopts. The `MUST avoid embedding secrets` doc strings are not enforced anywhere.

3. **[M1] Thread the real `step_index` into `HookedTool`** via a `task_local!` set by the inner agent loop, mirroring how `IN_BACKGROUND_TASK` already works in tasks.rs. Add an integration test that asserts the approval event step_index matches the corresponding agent step.

4. **[M2, M3] Close the trace-replay correctness gaps in the approval path** — emit synthetic events on the `stop_after_deny` short-circuit (M2) and preserve the original `request_id` in cached decisions (M3). Both are small code-shape fixes that restore replay determinism for the audit story.

5. **[M5] Decide on dispatch-error semantics for `SinkChain`** — either change to fail-soft (log + continue) or surface a per-sink `DispatchReport`. Today's behaviour aborts the run on any single sink error.

6. **[M6, M7] Tighten Cargo features and default profile** — explicit `tokio = ["time"]` and a typed acknowledgement for the permissive `Local` profile. Both are hardening for Beta consumers.

7. **[M4] Update CLAUDE.md and `tracing_bridge.rs` rustdoc** to acknowledge the unbridged `ExecutionTrace` gap, or close it with a small `HarnessEventListener` adapter.

8. **[m1-m12] Address minor issues** in a batch: document undocumented Beta wire-surface fields, replace the `expect()` with `unreachable!()` per project style, and stabilize the env-mutation tests in `tracing_bridge.rs`.

End of report.
