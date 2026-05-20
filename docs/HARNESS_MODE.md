# Harness Mode — Implementation Spec

Last updated: 2026-05-15
Status: **Phase H0 + H1 + H2 + H3 + H4 + H5 closed.** Slice 4 wrapped up the `:resume` route, swapped the Web UI from polling to SSE, and added the full-stack `tests/harness_full_stack_e2e.rs`. The follow-up append-mode resume slice is also in: the upstream contract knob `HarnessRuntime::with_initial_seq` is plumbed through the server, and the `:resume` route accepts `mode: "rerun" | "append"` so callers can preserve the prior event log and continue the seq series instead of restarting from `0`.

Harness Mode is AgentFlow's long-lived, workspace-aware agent session
layer. It wraps existing `AgentRuntime`, `ToolRegistry`, `SkillBuilder`,
memory, and tracing surfaces with a stable session protocol so the same
contract works across CLI direct execution, the local server, and the
embedded Web UI.

This document is the **implementation spec** owned by Phase H0+ work in
`TODOs.md` (segment `P-H`). The longer rationale that motivates the
design lives in `HARNESS_MODE_EVOLUTION.md`.

## Scope of this freeze (Phase H0)

Phase H0 freezes the **contract surface** so downstream consumers can
build against stable types while Phase H1 wires runtime execution. The
crate `agentflow-harness` ships these types only — no runtime, no
orchestration, no platform side effects.

Frozen surfaces:

- `HarnessEvent` — line-delimited JSON envelope for session activity.
- `ApprovalRequest` / `ApprovalDecision` — interactive approval
  protocol.
- `PreToolHook` / `PostToolHook` — async hook traits.
- `ApprovalProvider` — pluggable approval source.
- `ContextProvider` — pluggable project context source.
- `HarnessContext` / `HarnessProfile` / `HarnessRuntimeKind` — session
  descriptor.

Stability tier: **experimental** until Phase H1 exercises them
end-to-end (`docs/STABILITY.md`).

Envelope schema version: `harness/1` (constant
`agentflow_harness::HARNESS_ENVELOPE_SCHEMA_VERSION`). Bump only on
breaking wire shape changes; additive optional fields and additive
event kinds keep the same version.

## Crate placement

`agentflow-harness` is a **new crate** under `agentflow-harness/`. Two
reasons:

1. **Additive boundary.** Building Harness as a crate next to
   `agentflow-agents` rather than a module inside it makes the wrapper
   pattern explicit: Harness composes the existing runtime, it does
   not replace it. This addresses HARNESS_MODE_EVOLUTION Risk 1 ("a
   parallel runtime") by physical separation.
2. **Light dependency footprint.** The contract crate depends only on
   `agentflow-tools` (for `ToolIdempotency`, `ToolPermission`,
   `ToolSource`) plus `serde` / `chrono` / `async-trait` / `thiserror`.
   This keeps the wire surface reusable from UIs and SDKs that should
   not pull in the entire agent stack.

Phase H1+ will add execution dependencies (`agentflow-agents`,
`agentflow-skills`, `agentflow-tracing`, ...).

## Event envelope

```json
{
  "seq": 0,
  "session_id": "abc",
  "ts": "2026-05-14T12:34:56Z",
  "kind": "session_started",
  "payload": { ... }
}
```

Serialization uses `tag = "kind", content = "payload",
rename_all = "snake_case"`. `seq` is monotonically increasing per
session and starts at `0`; consumers reconnect with `after_seq=N`.

The frozen kind set (Phase H0):

| kind | payload | when emitted |
| --- | --- | --- |
| `session_started` | workspace, runtime, profile, model, skills, context summary | once at session bootstrap |
| `step_started` | step_index, step_type | start of each agent step |
| `tool_call_requested` | step_index, tool, source, permissions, idempotency, params_summary | agent asked to call a tool |
| `approval_requested` | embedded `ApprovalRequest` | policy or hook gated the call |
| `approval_decided` | embedded `ApprovalDecision` | provider returned a decision |
| `tool_call_completed` | step_index, tool, is_error, duration_ms, source, output_summary | tool returned |
| `background_task_updated` | task_id, status, summary, error | managed task state change |
| `memory_summary_added` | layer, summary, token_estimate | memory compaction appended a summary |
| `stopped` | reason, final_answer, error | session terminating |

The enum is **closed**. New kinds are additive AgentFlow releases.
Trace replay tooling depends on the closed surface.

## Approval protocol

```rust
pub struct ApprovalRequest {
  pub id: String,
  pub session_id: String,
  pub step_index: usize,
  pub tool: String,
  pub source: Option<ToolSource>,
  pub permissions: Vec<ToolPermission>,
  pub idempotency: ToolIdempotency,
  pub params_summary: serde_json::Value,
  pub risk: ApprovalRisk,
  pub reason: String,
  pub requested_at: DateTime<Utc>,
  pub expires_at: Option<DateTime<Utc>>,
}

pub enum ApprovalRisk { Low, Medium, High, Critical }

pub struct ApprovalDecision {
  pub request_id: String,
  pub decision: ApprovalOutcome,  // allow | deny | deny_and_stop
  pub scope: ApprovalScope,       // once | session | run
  pub decided_by: String,
  pub decided_at: DateTime<Utc>,
  pub reason: Option<String>,
}
```

Rules:

- `ApprovalRequest.id` is unique within a session; it is the join key
  between request and decision.
- `params_summary` MUST be redacted/truncated; raw secrets must not
  enter the wire envelope (HARNESS_MODE_EVOLUTION Risk 4).
- `expires_at` is honored by the provider; a missed deadline produces
  `HarnessError::ApprovalTimeout`, never an implicit allow
  (HARNESS_MODE_EVOLUTION Risk 2).

## Hook traits

```rust
#[async_trait]
pub trait ContextProvider: Send + Sync {
  fn name(&self) -> &str;
  fn priority_hint(&self) -> ContextPriority { ContextPriority::Normal }
  async fn collect(&self, ctx: &HarnessContext) -> Result<Vec<ContextItem>, HarnessError>;
}

#[async_trait]
pub trait PreToolHook: Send + Sync {
  fn name(&self) -> &str;
  async fn before_tool(&self, call: &PendingToolCall) -> Result<PreToolDecision, HarnessError>;
}

#[async_trait]
pub trait PostToolHook: Send + Sync {
  fn name(&self) -> &str;
  async fn after_tool(&self, call: &CompletedToolCall) -> Result<(), HarnessError>;
}

#[async_trait]
pub trait ApprovalProvider: Send + Sync {
  fn name(&self) -> &str;
  async fn request(&self, request: ApprovalRequest) -> Result<ApprovalDecision, HarnessError>;
}
```

`PreToolDecision` is a tagged enum (`allow` / `require_approval` /
`deny`). The runtime composes multiple `PreToolHook`s; the strictest
returned decision wins.

`PostToolHook` is advisory: a hook failure is recorded but never rolls
back the tool call.

`ContextProvider`s must be deterministic for a given context when no
external state has changed, so trace replay can reproduce the prompt.
Providers emit structured `ContextItem`s with priority and token cost;
the runtime composes them under a token budget rather than dumping
files blindly (HARNESS_MODE_EVOLUTION Risk 4).

### Wiring hooks into the tool registry (Phase H2)

The `agentflow_harness::wrap_registry` function decorates every tool
already registered in a `ToolRegistry` with a `HookedTool` wrapper.
The wrapper:

1. Builds a `PendingToolCall` from the tool metadata + params.
2. Runs every registered `PreToolHook` under a bounded timeout
   (`DEFAULT_HOOK_TIMEOUT = 5s`; configurable via
   `HookConfig::with_hook_timeout`). Pre-hook timeouts and errors
   are fail-closed — the call is denied with a reason that names the
   offending hook.
3. Merges the per-hook decisions (`Deny` > `RequireApproval` >
   `Allow`).
4. **Production escalation**: when `HarnessProfile::Production` is
   active and the call's idempotency is `NonIdempotent`, the wrapper
   escalates even an `Allow` to `RequireApproval` with risk
   `Critical` so production runs are fail-closed by default
   (HARNESS_MODE_EVOLUTION Risk 2).
   - **Footgun (F-A2-12)**: under `HarnessProfile::Local` (the
     `Default`) and `HarnessProfile::Dev`, this escalation does
     **NOT** happen. Without an explicit pre-hook returning
     `RequireApproval`, mutating tools are silently auto-allowed
     and the `ApprovalProvider` is never invoked. If you wire
     `wrap_registry` expecting "the approval prompt should fire on
     every shell / file:write call", you MUST pair it with
     `.with_profile(HarnessProfile::Production)` (or register a
     pre-hook that returns `RequireApproval`). The example below
     does this; copy that line verbatim.
5. If approval is required: emits `HarnessEvent::ApprovalRequested`,
   delegates to the configured `ApprovalProvider`, emits
   `ApprovalDecided`. `Session` / `Run` scope decisions are cached
   per tool name so subsequent calls reuse the prior outcome without
   re-prompting. `DenyAndStop` short-circuits every subsequent call
   without raising further approvals.
6. Dispatches the inner tool when allowed, returns
   `ToolError::PolicyDenied` otherwise.
7. Runs every `PostToolHook` (advisory; failures are logged but
   never undo the tool result).

Three reference providers ship in `agentflow_harness::approval_providers`:

- `AutoAllowApprovalProvider` — CI smoke + dev profile override.
- `AutoDenyApprovalProvider` (`with_stop_on_deny`) — production
  fail-closed default.
- `CliApprovalProvider` — blocking stdin prompt with explicit scope
  parsing (`y` / `s`ession / `r`un / `n` / `q`uit). Honours
  `ApprovalRequest::expires_at` by racing
  `tokio::time::sleep` against `spawn_blocking` stdin.

Usage pattern:

```rust
let mut registry = ToolRegistry::new();
registry.register(Arc::new(ShellTool::new(policy.clone())));

let sinks = SinkChain::new().push(jsonl_sink);
let approval = Arc::new(CliApprovalProvider::stdin());
let hooked_registry = wrap_registry(
  registry,
  HookConfig::new("sess-1", approval, sinks.clone())
    // Load-bearing: without Production (or an explicit pre-hook
    // returning RequireApproval), Local profile silently auto-allows
    // every NonIdempotent call and the ApprovalProvider above is
    // never invoked. See F-A2-12.
    .with_profile(HarnessProfile::Production)
    .with_pre_hook(my_audit_hook)
    .with_hook_timeout(Duration::from_secs(2)),
);

let agent = ReActAgent::new(config, memory, Arc::new(hooked_registry));
let mut runtime = HarnessRuntime::new(Box::new(agent))
  .with_event_sink(jsonl_sink);
runtime.run(options).await
```

For a fully-runnable reference binary that wires this exact pattern
(plus `--auto-approve` / `--prefetch-diff` modes for CI smoke and
write-side dogfooding), see
[`examples/applications/code-reviewer-write/`](../examples/applications/code-reviewer-write/README.md).

`HookedTool` only emits approval-lifecycle events. Tool-call
lifecycle events (`tool_call_requested` / `tool_call_completed`)
keep flowing from the `HarnessRuntime` post-hoc translation, so
existing consumers do not see duplicates when hooks are wired.

## Session context

```rust
pub struct HarnessContext {
  pub session_id: String,
  pub workspace_root: PathBuf,
  pub user_input: String,
  pub model: String,
  pub runtime: HarnessRuntimeKind,
  pub profile: HarnessProfile,
  pub metadata: serde_json::Value,
}

pub enum HarnessRuntimeKind {
  React,
  PlanExecute,
  Handoff,
  Blackboard,
  Debate,
}

pub enum HarnessProfile { Dev, Local, Production }
```

Phase H1 will populate these inside `HarnessRuntime::start` and pass
them to providers and hooks.

## CLI surface (shipped in Phase H1)

```bash
agentflow harness run "Analyze this project and propose next steps"
agentflow harness run --skill ./skills/code-review "Review current changes"
agentflow harness run --output stream-json "Implement the next TODO safely"
agentflow harness resume <session_id>
agentflow harness list
agentflow harness inspect <session_id>
```

Flags:

```text
--model <model>
--runtime react|plan_execute|handoff|blackboard|debate
--skill <path-or-name>
--mcp-config <path>
--permission-mode ask|deny|auto
--security-profile dev|local|production
--output text|json|stream-json
--run-dir <path>
--trace-dir <path>
```

Initial implementation must not ship a TUI. A stable `stream-json`
event surface gives TUI / Web UI a clean integration point later
(HARNESS_MODE_EVOLUTION Risk 5).

The CLI also exposes `agentflow harness list` / `agentflow harness
inspect <session_id>` for offline session log triage. Both honour the
same `--run-dir` precedence (explicit → `AGENTFLOW_RUN_DIR` →
`AGENTFLOW_TRACE_DIR` → `~/.agentflow/runs`) so the trace replay tools
can find Harness logs without bespoke wiring.

### Tracing bridge

`agentflow_harness::tracing_bridge` resolves the session-log root from
the `AGENTFLOW_TRACE_DIR` convention shared by the rest of AgentFlow
trace tooling. Each Harness session is one append-only JSONL file at
`<base>/harness/sessions/<session_id>.jsonl`. Deeper integration with
`agentflow-tracing::TraceStorage` (a single storage layer for both
agent and Harness events) is Phase H5 work; it does not block Phase
H1 because the on-disk layout already makes the data discoverable.

## Server surface (Phase H5)

Phase H5 slice 1 (closed) ships the schema + core lifecycle routes:

```text
POST /v1/harness/sessions                       # closed (slice 1)
GET  /v1/harness/sessions                       # closed (slice 1)
GET  /v1/harness/sessions/{id}                  # closed (slice 1)
POST /v1/harness/sessions/{id}:cancel           # closed (slice 1)
GET  /v1/harness/sessions/{id}/events           # closed (slice 1) — SSE with backfill
GET  /v1/harness/sessions/{id}/events/history   # closed (slice 1) — JSON history
```

Phase H5 slice 2 (closed) adds the approval surface + the real
LLM-backed executor:

```text
GET  /v1/harness/sessions/{id}/approvals        # closed (slice 2)
POST /v1/harness/sessions/{id}/approvals/{id}   # closed (slice 2)
```

Phase H5 slice 3 (closed) ships the Web UI surface:

- `/ui/harness/sessions` — tenant-scoped session list (auto-refresh).
- `/ui/harness/sessions/new` — submit form (prompt + workspace_root +
  profile + runtime + model + skill_name; localStorage-persisted
  inputs minus the API token).
- `/ui/harness/sessions/{id}` — detail page with summary, event
  timeline (kind-tone colours + payload pane), pending approval cards
  with allow / deny / deny_and_stop × `once` / `session` / `run` scope
  dropdown, and a cancel button that disables once the session is
  terminal.

Phase H5 slice 4 (closed) finishes the surface:

```text
POST /v1/harness/sessions/{id}:resume           # closed (slice 4)
```

`:resume` accepts a `mode` field on the request body that selects
between two semantics:

- **`mode: "rerun"`** (default; preserves backwards compat for callers
  that omit the field): DELETE every persisted event for the session
  in one Postgres transaction, flip the row back to `running`,
  optionally replace `user_input`, and respawn the executor with
  `initial_seq = 0`. Useful for retry-with-tweak debugging or
  replaying after a transient LLM failure.
- **`mode: "append"`**: keep the prior event log intact, flip the row
  back to `running`, optionally replace `user_input`, look up
  `MAX(seq)` from `harness_session_events`, and spawn the executor
  with `initial_seq = MAX(seq) + 1`. The new run extends the
  persisted timeline (consumers see `seq` continue past the previous
  terminal event) instead of starting a fresh series. Natural shape
  for follow-up instructions and for resuming a forced cancel
  without losing the trace.

Both flavours echo the applied `mode` in the response body so callers
that omit the field can confirm the default.

`POST /v1/harness/sessions/{id}:cancel` and
`POST /v1/harness/sessions/{id}:resume` share one Axum POST handler
(`post_harness_session_action`) that dispatches on the suffix —
Axum can't bind two POST handlers to the same path pattern, so the
dispatcher is the cleanest way to keep two semantically distinct
actions on a single REST resource.

Slice 4 also flips the Web UI detail page from polling to
`EventSource` SSE, with a 5 s history-poll fallback for clients that
lose the broker channel (the workflow `EventBroker` contract drops
long-completed sessions to keep memory bounded). A stream-state pill
in the controls strip surfaces `streaming` / `error` / etc. A
**Resume** button appears once the session is terminal; the operator
can optionally provide a new prompt and pick a mode from a `rerun` /
`append` dropdown next to the button. The button label echoes the
selected mode (`Resume (rerun)` / `Resume (append)`). In rerun the
client clears the local timeline so stale events don't show while the
executor reproduces them; in append the prior events stay visible
because the new seqs arrive on top of them as a single continuous
timeline.

The combined integration test
`agentflow-server/tests/harness_full_stack_e2e.rs` exercises every
layer the Web UI consumes in one ~6.5 s pass against real Postgres +
Moonshot: submit → SSE stream → DB history → terminal row → resume
→ rerun history. Skips automatically without
`AGENTFLOW_DATABASE_TEST_URL` and `MOONSHOT_API_KEY`.

DB schema (slice 1): two dedicated tables `harness_sessions` and
`harness_session_events` (Postgres migration
`0002_harness_sessions.sql`). The lifecycle columns diverge enough
from workflow `runs` (workspace root, security profile, runtime kind,
model, optional skill) that overloading the existing schema with a
`kind` discriminator + sentinel nullables would be strictly worse
than two narrow tables. SSE subscribers use `(session_id, seq)` for
backfill/replay; channel reuse for the `harness_session_events` log
mirrors the workflow `events` contract exactly.

Slice 1 plumbing uses a `StubHarnessExecutor` that emits
`session_started` + `stopped` events and marks the session row as
`failed: executor_not_yet_wired`. Slice 2 introduces
`LiveHarnessExecutor`, which wires `HarnessRuntime` ↔ `ReActAgent` ↔
a hook-wrapped tool registry (`wrap_registry(HookConfig)`) backed by
`ServerApprovalProvider`. `agentflow serve` swaps the default stub
for the live executor; unit tests keep the stub via plain
`AppState::new(db)` so the hermetic test suite never contacts an LLM
provider. `HarnessRuntime::run` holds `&self` across awaits, so the
live executor runs every session on its own current-thread Tokio
runtime hosted in `tokio::task::spawn_blocking` to preserve `Send` at
the outer trait boundary.

The server is **optional**. CLI direct execution stays first-class.

## Fixtures and tests

Phase H0 ships frozen-fixture round-trip tests in
`agentflow-harness/tests/`:

- `envelope_contract.rs` — decode + re-encode of session bootstrap,
  approval request/decision, terminal stopped event, and an additive
  unknown-field tolerance check.
- `tests/fixtures/*.json` — the v1 wire fixtures. Changes to these
  files must bump `HARNESS_ENVELOPE_SCHEMA_VERSION` and update
  `docs/STABILITY.md`.

Phase H1 will add execution tests (session bootstrap, ReAct
integration, context provider determinism); Phase H2 will add
approval lifecycle tests (allowed once, allowed for session, denied,
cancelled, timeout).

## Phase plan recap

`HARNESS_MODE_EVOLUTION.md` carries the full plan. `TODOs.md` segment
`P-H` is the operational queue. The current ordering:

| Phase | TODO id | Theme | Status |
| --- | --- | --- | --- |
| H0 | P-H.0 | Contract inventory (this doc) | **closed** |
| H1 | P-H.1 | Runtime MVP, default providers, CLI entry | **closed** |
| H2 | P-H.2 | Hooks + approval (`wrap_registry`, 3 providers, production fail-closed) | **closed** |
| H3 | P-H.3 | Parallel native tool calls (ReAct batch dispatcher, deterministic LLM-order trace) | **closed** |
| H4 | P-H.4 | Background task tools (`TaskRuntime` + 5 `task_*` tools, nested-spawn rejection, bounded output buffer) | **closed** |
| H2 | P-H.2 | Hooks + approval (depends on P1.7 resume policy) | gated |
| H3 | P-H.3 | Parallel native tool calls (depends on P3.7) | gated |
| H4 | P-H.4 | Background task tools | gated |
| H5 | P-H.5 | Server + Web UI (depends on P2.1, P2.2, P2.4, P6.1) | **closed** |
| H6 | P-H.H6 | Advanced compatibility (TUI, plugin adapters) | deferred — see `docs/H6_PROMOTION_CRITERIA.md` for per-item promotion triggers |

## Architectural invariants

These are enforced via PR review:

1. **Wrap, do not replace.** Harness Mode wraps existing `AgentRuntime`,
   `ToolRegistry`, trace contracts, and Skill contracts. New behavior
   is additive through hooks, events, and session context.
2. **Default to ask / deny for mutating tools** in `local` and
   `production` profiles. `dev` profile may auto-allow but must still
   emit the approval lifecycle events.
3. **Resume honors `ToolIdempotency`.** Non-idempotent tool calls
   require explicit manual recovery (`P1.7`). The runtime must never
   silently replay an `Unknown` or `NonIdempotent` call.
4. **Context items carry priority and token cost.** Providers MUST
   NOT dump entire files into the prompt without budget control.
5. **UI is a client of the protocol.** The server SSE surface and Web
   UI rendering are downstream consumers of `HarnessEvent`; they do
   not get to invent their own wire shape.

## Open questions deferred to Phase H1

- Where does `HarnessSessionStore` live (in-memory only vs. backed by
  the existing `runs` schema with a `kind = harness` column)?
- How are background-task lifecycle events deduplicated when the same
  task transitions multiple times?
- Default token budget per `ContextPriority` band — concrete numbers
  pending real prompt assembly tests.

These do not block the H0 freeze; they are recorded here so Phase H1
work can pick them up directly.
