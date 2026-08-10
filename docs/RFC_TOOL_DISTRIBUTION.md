# RFC: Tool Distribution Contract (W4.1)

- Status: **Proposed** — design only; implementation is sequenced into four
  sub-items (W4.1a–d) below, each landing independently.
- Parent: `TODOs.md` W4.1 ("工具分发契约 RFC + 落地"), the highest-ROI
  unblocked item in the 2026-08-09 evaluation's W4 (platform
  productionization) segment.
- Scope: three concrete gaps in how a tool registry crosses a process/request
  boundary — no change to the `Tool` trait shape or any existing wire
  contract's field names.

## Problem

The codebase has no unified contract for "distribute a tool registry across
a boundary" (manifest serialization + sandbox-policy propagation + approval
routing). Three previously-unrelated-looking gaps share this one root cause:

**Gap A — skill runs execute with zero approval gating.**
`agentflow-server/src/runs.rs::run_skill_agent` (lines 657–700+, invoked from
`skill_execute` at line 595, itself the backing for `POST
/v1/skills/{name}:run`) builds a skill's agent via
`SkillBuilder::build(&manifest, skill_dir)` (line 673) and runs it directly.
Unlike every other tool-execution path in this codebase — the CLI's `harness
run`/`chat` (`agentflow-cli/src/commands/harness/run.rs:190-199`), and the
server's own harness-session path (`harness_live.rs:510`/`616`) — nothing
wraps the resulting registry with `wrap_registry`/`HookConfig`. A skill
declaring `shell`/`script`/`code_exec` tools (real OS-level effect tools)
runs those tools ungated under `/v1/skills/{name}:run`, with no
operator-visible record of what capabilities the run actually exercised.

**Gap B — harness sessions can't use a skill's tools.**
`agentflow-server`'s harness-session creation path accepts a `skill_name` on
the session-creation request (`harness.rs:224`, `HarnessSessionContext.
skill_name` at line 324) and threads it end-to-end
(`harness.rs:496,509,516,824,835,985`) — but `harness_live.rs` only ever
reads it as a display label passed to
`AgentContextOptions::with_skill_name` (line 546-547). The module's own doc
comment says so explicitly: *"Skill-backed tool loading (`inputs.skill_name`)
is not wired yet — the runtime just carries the name for observability, the
full form (tool distribution contract, W4.1) is tracked separately"*
(`harness_live.rs:381-382`). A server-hosted harness session can never get a
skill's declared tools — only the hardcoded `FileTool::read_only` +
`HttpTool` from `build_default_tool_registry` (`harness_live.rs:386`).

**Gap C — distributed `agent` DAG nodes always get an empty registry.**
`agentflow-worker/src/lib.rs::execute_agent_payload` (line 897) always
constructs its `ReActAgent` with `Arc::new(ToolRegistry::new())` (line 915)
— permanently empty. The worker is a genuinely separate process (possibly a
separate machine); nothing today lets the DAG author declare which tools an
`agent` node should have, or lets the worker build them safely from
untrusted-origin config.

## Empirical inventory (what's reusable vs. missing, verified against the current tree)

| Item | Location | State |
|---|---|---|
| `ToolDefinition` | `agentflow-tool/src/tool.rs:129-135` | serde-ready (`name`, `description`, `parameters: Value`, `metadata`); describes **one** tool, no aggregate/registry-level DTO exists |
| `Capability` / `EffectiveCapabilities` | `agentflow-tool/src/capability.rs:19,75,101,116` | serde-ready (`Serialize, Deserialize` derives present); multi-layer merge algorithm already implemented and used by the harness/hooks pipeline |
| `ApprovalRequest` / `ApprovalDecision` | `agentflow-agent-spi` (re-exported via `agentflow_harness`) | serde-ready; **already crosses one process boundary today** via `ServerApprovalProvider` (`agentflow-server/src/harness_approval.rs:186-240`) parking requests on a `PendingApprovalRegistry` (line 56) that the `GET/POST /v1/harness/sessions/{id}/approvals*` routes (`harness_approval.rs:244,299`, registered at `lib.rs:590-597`) resolve from the HTTP side |
| `SandboxPolicy` | `agentflow-tools/src/sandbox/policy.rs:29` | **not** serde-ready — `#[derive(Debug, Clone)]` only, no `Serialize`/`Deserialize` |
| `ReActAgent::tools()` / `with_tools()` | `agentflow-agents/src/react/agent.rs:595,611` | already the registry-swap hook: snapshot the built agent's registry, wrap it, swap it back in. Already used in production by the CLI (`agentflow-cli/src/commands/harness/run.rs:194-199`): `let mut snapshot = ToolRegistry::new(); for tool in agent.tools().list() { snapshot.register(tool); } let wrapped = wrap_registry(snapshot, hook_config); agent = agent.with_tools(Arc::new(wrapped));` |
| `execute_file_payload`'s `allowed_paths` | `agentflow-worker/src/lib.rs:792-813` | direct precedent for Gap C: a worker payload already reads a tool-shaping field (`allowed_paths`) out of `payload.parameters` (a plain JSON bag) rather than needing a `worker.proto` change |
| `SkillCatalog::resolve` | `agentflow-server/src/skills.rs:109` | returns `Option<ResolvedSkillRegistryEntry { path: PathBuf, .. }>` (`agentflow-skills/src/index.rs:41-49`) — already used by the sibling `/v1/skills` route; directly reusable for Gap B's session-creation handler |
| `SkillBuilder::build` / `build_with_project_root` | `agentflow-skills/src/builder.rs:46,78` | both already exist; `build_with_project_root` is the CLI-parity constructor (persona/knowledge/memory/project-memory all wired) |
| `publish_through` (run-scoped event persistence) | `agentflow-server/src/events_stream.rs:184` | existing helper `skill_execute` already calls to write into the run's own `events` table — the natural destination for Gap A's tool-call/approval events, so they surface on the `/v1/runs/{id}/events` SSE stream operators already watch |
| `AppState.approval_registry` | `agentflow-server/src/lib.rs:139` | `PendingApprovalRegistry`, keyed generically by `(session_id: String, request_id: String)` (`harness_approval.rs:65`) — nothing session-specific about the key shape, so `run_id.to_string()` works as the first element without any change to `PendingApprovalRegistry` itself |

**Conclusion from the inventory:** Gaps A and B need **no new wire protocol**
— both are wiring problems solvable entirely with existing types
(`wrap_registry`, `HookConfig`, `ServerApprovalProvider`,
`PendingApprovalRegistry`, `SkillBuilder`, `SkillCatalog::resolve`) that
already work together in the harness-session path. Gap C is the only
genuinely novel piece: it needs an aggregate tool-manifest DTO (none exists
today) and a manifest → `ToolRegistry` builder, both of which fit inside
`payload.parameters`'s existing JSON bag with no `worker.proto` change for a
File+Http-only first cut.

## Decision

**The manifest DTO lives in `agentflow-tools` (L2), not the L0
`agentflow-tool` kernel crate.** All three surface crates that would consume
it — `agentflow-cli`, `agentflow-server`, `agentflow-worker` — already
depend on `agentflow-tools` directly (for the concrete `FileTool`/`HttpTool`
implementations the manifest resolves into), so pushing the DTO into the
kernel crate buys no isolation. The "which builtin does `kind: File` map to"
logic is inherently impl-tier, matching why `RFC_TOOL_CONTRACT_SPLIT.md`'s
T3.3 moved concrete tools *out* of the kernel crate in the first place — a
manifest resolver is exactly that kind of impl-tier logic, just running in
reverse (DTO → concrete tool instead of concrete tool → DTO).

Sketch, `agentflow-tools/src/manifest.rs` (new file, sibling to
`defaults.rs`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuiltinToolKind { File, Http }  // closed enum — File+Http only this pass

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManifestEntry {
  pub kind: BuiltinToolKind,
  pub definition: ToolDefinition,     // reused from agentflow_tool::tool
  #[serde(default)]
  pub required: Vec<Capability>,      // reused from agentflow_tool::capability; declared for audit only this pass, not merged against a policy layer (no skill/policy layer reaches the worker in this design yet)
  #[serde(default)]
  pub sandbox: Option<SandboxPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManifest { pub tools: Vec<ToolManifestEntry> }

pub fn build_registry_from_manifest(
  manifest: &ToolManifest,
  workspace_root: &Path,
) -> Result<ToolRegistry, ToolError>
```

`build_registry_from_manifest` matches on `kind`, constructs the concrete
tool (`FileTool::read_only`/`FileTool::new` + `HttpTool::new`, sourcing
paths/domains from `entry.sandbox`) — the same shape as the existing
`default_governed_registry` but manifest-driven instead of hardcoded.

Gaps A and B need no new type at all — both reuse the CLI's existing
snapshot-registry → `wrap_registry` → `with_tools` three-liner verbatim, and
Gap A reuses `ServerApprovalProvider` + `PendingApprovalRegistry` exactly as
the harness-session path does today, just keyed by `run_id` instead of
`session_id`.

## Sequencing

Four independently shippable sub-items, each its own
investigate → implement → test → commit → `TODOs.md`-update cycle:

1. **W4.1a** (this document) — RFC only, no code.
2. **W4.1b — Gap A: skill-run approval wrapping.**
   `agentflow-server/src/runs.rs` (`run_skill_agent`, `skill_execute`,
   `RunContext`): thread `approval_registry: PendingApprovalRegistry` +
   an approval-timeout `Duration` into `run_skill_agent` (sourced the same
   way `LiveHarnessExecutor::new` does, `harness_live.rs:151,172`); after
   `SkillBuilder::build` returns the agent, build a `ServerApprovalProvider`,
   a small new `RunHarnessEventSink` (writes into the run's `events` table
   via `events_stream::publish_through`, mirroring `ServerHarnessEventSink`
   at `harness_live.rs:57-94` but targeting a different table), a
   `HookConfig`, and apply the snapshot → `wrap_registry` → `with_tools`
   pattern. Add `GET /v1/runs/{id}/approvals` + `POST
   /v1/runs/{id}/approvals/{request_id}` routes, handlers mirroring
   `list_pending_approvals`/`decide_approval`
   (`agentflow-server/src/harness_approval.rs:244,299`) but keyed by
   `run_id` against the same `AppState::approval_registry`.
3. **W4.1c — Gap B: harness-session skill-dir resolution.**
   `agentflow-server/src/harness.rs` (session-creation handler): when
   `req.skill_name` is `Some`, call `state.skills.resolve(name)` to get a
   `PathBuf`; add `skill_dir: Option<PathBuf>` to `HarnessSessionContext`,
   threaded the same way `skill_name` already is. In
   `agentflow-server/src/harness_live.rs`'s registry-construction sites
   (lines 510 and 616): when `inputs.skill_dir` is `Some`, load+validate the
   manifest and call `SkillBuilder::build_with_project_root`, then apply the
   same snapshot → `wrap_registry` → `with_tools` pattern as W4.1b instead
   of the current `build_default_tool_registry` path; `None` keeps today's
   behavior unchanged.
4. **W4.1d — Gap C.1+C.2: worker tool manifest (File+Http).**
   C.1: add `Serialize`/`Deserialize` derives to `SandboxPolicy`
   (`agentflow-tools/src/sandbox/policy.rs:29`) + a round-trip test. C.2:
   new `agentflow-tools/src/manifest.rs` per the sketch above; in
   `agentflow-worker/src/lib.rs::execute_agent_payload` (line 897), read an
   optional `payload.parameters["tools"]`, deserialize as `ToolManifest`
   when present, call `build_registry_from_manifest` instead of
   `ToolRegistry::new()` (line 915); absent field keeps today's
   empty-registry behavior. Confirm
   `agentflow-server/src/scheduler/distributed.rs::dispatch_node` needs no
   change (forwards `parameters` verbatim) — verify directly rather than
   trust this note, it is a one-line grep.

Each sub-item keeps `cargo test`/`clippy -D warnings`/`fmt --check` green for
every crate it touches, plus `cargo xtask check-arch` (the manifest DTO
living in `agentflow-tools`, an L2 crate, is not expected to trip any
dependency law, but the gate is re-run to confirm) and a full `cargo build
--workspace --all-targets` before each commit.

## Effort estimate and risk

- **W4.1b/c are wiring, not design** — every type and function they call
  already exists and is already proven in production by the CLI's
  `harness run` path and the harness-session path itself. Main risk is
  purely mechanical: getting the `RunContext`/`HarnessSessionContext`
  threading right so the new fields reach the right call site.
- **W4.1d is the one genuinely new surface** — `ToolManifest` is a new
  wire-facing DTO. Scoped deliberately narrow (File+Http only) to keep the
  first cut low-risk; the closed `BuiltinToolKind` enum makes extending it
  later (Shell/Script/CodeExec) an additive, non-breaking change.
- **Explicitly deferred, not designed here:**
  - Shell/Script/`code_exec` manifest entries — these are real
    effect-producing tools; distributing them to a worker process needs an
    approval-routing story analogous to Gap A/B's, which in a distributed
    worker means a `WorkerControl` RPC (the worker has no HTTP client back
    to the gateway's approval routes today). Out of scope until a worker
    approval RPC is designed.
  - `WorkerControl` approval RPC itself — would let a worker-side tool call
    request approval from the gateway the way `ServerApprovalProvider` does
    in-process today. Needs its own RFC once Shell/Script/CodeExec
    distribution is prioritized.
  - Capability-based policy enforcement of `ToolManifestEntry.required` — declared
    for audit visibility in W4.1d's scope, not merged against any policy
    layer, since no skill/policy layer reaches the worker in this design.
