# RFC: Crate Architecture — Contract Kernel + Strangler Evolution

- Status: **Proposed** — design accepted; execution tracked in `TODOs.md` §P-A.
- Author: (proposed)
- Created: 2026-06-19
- Related: `docs/RFC_HARNESS_LOOP_OWNERSHIP.md`, `docs/ARCHITECTURE.md`,
  `docs/EXTENSIBILITY_MODEL.md`, `RoadMap.md`,
  `docs/archive/PROJECT_EVALUATION_2026-06-06.md`, `CLAUDE.md` (four-layer model)
- Affected crates: workspace-wide (new contract crates + dependency re-pointing).
  **No public-surface change** — CLI commands, workflow YAML schema, `SKILL.md`,
  and HTTP `/v1/*` contracts are all preserved.

## TL;DR

AgentFlow today supports four execution paradigms — static DAG workflows, native
agent loops, the harness governance shell, and (next) **dynamic workflows** (an
agent that *generates* a `Flow` at runtime). They mostly coexist, but three
seams keep them as semi-integrated parallel code paths instead of one coherent
stack:

1. `agentflow-harness` depends on the `agentflow-agents` **crate** (not a
   contract), so the harness cannot govern anything but an agent loop.
2. Reliability primitives (retry / timeout / cancellation) are implemented twice
   — once in `agentflow-core`, once in the `agentflow-agents` loop.
3. The DAG execution IR (`Flow`) is fused with the executor, so a runtime cannot
   *construct* a `Flow` without depending on the scheduler — which blocks
   dynamic workflows.

This RFC defines a **narrow-waist contract kernel** plus eight dependency laws,
and reaches it by an **in-place strangler-fig migration — explicitly NOT a
rewrite**. The end-state makes dynamic workflow nearly free (no new sideways
dependency) and turns the layering from convention into compiler-enforced
structure via Rust's trait system.

## 0. Decision — refactor in place, do NOT rewrite

A rewrite is justified only when no incremental path exists. None of the rewrite
signals apply here:

| Rewrite signal | AgentFlow reality |
|---|---|
| Foundational tech choice is wrong (lang/runtime/data model) | ❌ Edition 2024, Tokio, `thiserror`, no-`unwrap` discipline — all sound |
| No incremental path; changing A always breaks B | ❌ Target is reached by extracting contract crates + re-pointing deps |
| Core abstractions cannot carry new requirements | ❌ The IR already carries dynamic workflow once IR≠executor is split |
| No tests, unowned, unchangeable | ❌ ~132K LOC, eval harness, live-LLM CI, actively evolved |

The ~80% of the design that is already correct (dependency-clean `core` / `tools`
/ `llm` foundations, the faithful `LoopSession` extraction, 6-provider
consistency suite, checkpoint fidelity) is an expensive, hard-won asset. We
**evolve in place** and keep the tree green and shippable at every step.

## 1. Guiding principle — unify contracts downward, keep drivers distinct

Unification happens at the **contract layer** (everything reduces to a small set
of narrow interfaces), **not** at the driver layer. Building one "do-everything"
orchestrator over the four paradigms would be the *big-and-broad* trap. Instead:

- **Contract kernel** = a narrow waist; everyone depends on it; it depends on no
  concrete implementation.
- **Four drivers** = independent; they never import each other; they share only
  the kernel.

Crate-boundary rule (prevents both god-crates and over-splitting): **a crate
exists iff its set of dependents is distinct.** Merge things with the same
dependent-set; split things whose dependent-sets differ.

## 2. Capability vs Tool — the two load-bearing traits

- **Tool (`工具`)** — an atomic, uniform-signature callable. The *only* thing a
  runtime loop directly invokes. Lives in a registry. Heterogeneous, injected.
- **Capability (`能力`)** — a packaged unit (persona + tools + knowledge + config)
  that **lowers** to *tools + context* at the runtime boundary.

```rust
// agentflow-tools — atomic action (object-safe SPI)
#[async_trait]
pub trait Tool: Send + Sync {
  fn metadata(&self) -> ToolMetadata;                 // source / idempotency / permissions
  async fn call(&self, params: FlowValue) -> Result<ToolOutput, ToolError>;
}

// agentflow-agent-spi — a capability lowers to (tools + context)
pub struct Lowered { pub tools: Vec<Arc<dyn Tool>>, pub context: Vec<ContextFragment> }
#[async_trait]
pub trait Capability: Send + Sync {
  async fn lower(&self) -> Result<Lowered, CapabilityError>;
}
```

`lower()` returns an **owned** `Lowered` (not `&mut Assembly`) so capabilities
compose by flatten and are trivially testable. The surface merges all `Lowered`
into one `ToolRegistry` + `ContextBuilder` and hands it to a runtime. The runtime
forever sees only **Tool + Context + AgentRuntime**.

**Status (P-A4.3):** `Capability` + `Lowered { tools, context }` +
`CapabilityError` live in `agentflow-agent-spi::capability`; `Lowered.context`
reuses the kernel's existing `ContextItem` (so the RFC's "ContextFragment" is
that type), and `Lowered::merge` is the flatten primitive. `agentflow-skills`
ships `SkillCapability`, which lowers a Skill to its tool registry contents + the
persona as a `Critical` context fragment. Full surface adoption (merging a
`Vec<Box<dyn Capability>>` in place of the direct `SkillBuilder::build` path) is
the remaining follow-up.

**Modeling rule:** open behavior extension points are `trait`s
(`Tool`/`Capability`/`AgentRuntime`/`KnowledgeBackend`); closed data sets we own
are `enum`s with `#[non_exhaustive]` (`FlowValue`, `HarnessEvent` kind,
`ToolOutputPart`). Never an event-as-trait or a tool-as-enum.

## 3. Target layering

| Tier | Representative crates | Single responsibility | May depend on | Must NOT depend on |
|---|---|---|---|---|
| Surfaces | `cli`, `server`, `worker`, `ui` | Assembly (DI): import everything, wire capabilities into runtimes | all | — |
| Runtimes | `core` (executor), `agents` (loop, can emit `Flow`), `harness` (governance) | Drive loop / execute graph / govern | **contracts only** | each other |
| Capabilities | `skills`, `rag`, `memory`, `llm` | `impl Capability` / `impl *Backend`; lower to tools+context | contracts + wrapped tools | runtimes |
| Tools | `tools-builtin`, `mcp`, `rag` (`rag_search`), `nodes` | `impl Tool` / provide `AsyncNode` | tool contract (+ backend) | runtimes, other tools |
| Contract kernel | `value`, `tool`, `graph`, `store-spi`, `agent-spi`, `async-util` | The narrow waist | only downward to `value` | any impl |

## 4. Contract kernel (the narrow waist)

| Crate | Single responsibility | Dependents | Own deps | Priority |
|---|---|---|---|---|
| `agentflow-value` | Data contract: `FlowValue` + conversions | almost all | none | defer (re-export) |
| `agentflow-tool`* | Action contract: `Tool` / `ToolRegistry` / `ToolMetadata` | tools, capabilities, runtimes | value | exists today |
| `agentflow-graph` | Execution IR: `AsyncNode` / `GraphNode` / `Flow` / `NodeType` | core, nodes, agents, worker | value | **must** (split from core) |
| `agentflow-store-spi` | `KnowledgeBackend` / `MemoryStore` | skills, rag, memory, agents | value | **must** |
| `agentflow-agent-spi` | `AgentRuntime` / `AgentEvent` / `Capability` / `HarnessEvent` / `EventSink` / `Approval*` | agents, harness, tracing, server | value, tool | **must** |
| `agentflow-async-util` | retry / timeout / cancellation combinators | core, agents | tokio, futures | must (de-dup) |

*`agentflow-tool` = the `Tool` contract carved out of today's `agentflow-tools`;
the built-in file/http/shell tools move to `agentflow-tools-builtin` (optional;
may stay feature-gated in place initially).

Safe merges are rare: `store-spi` cannot fold into `agent-spi` (that would force
`rag` to depend on `AgentRuntime`); `value` stays a separate universal leaf. The
crate count reflects genuinely distinct dependent-sets — this is evidence of low
coupling, not ceremony.

## 5. IR ≠ executor

`Flow` (the DAG *type*) lives in `agentflow-graph`; the topological/concurrent
scheduler `FlowExecutor` lives in `agentflow-core`. This single split lets
`agents` depend on `graph` to **construct** a `Flow` (the dynamic-workflow
prerequisite) without ever depending on the executor — preserving "runtimes
never depend on each other."

## 6. Rust trait design decisions

1. **dyn at seams, generic inside.** Cross-injection contracts are object-safe
   (`Arc<dyn Tool>`, `Box<dyn Capability>`, `Box<dyn AgentRuntime>`). Internal
   hot paths use generics/concrete types (monomorphized, zero-cost).
2. **Object-safety discipline.** The four SPI traits avoid object-safety killers
   (no generic methods, no `Self` return, no associated types in signatures);
   `async fn` is boxed via `#[async_trait]` at the seam only. Concrete returns
   (`Vec<Chunk>`), never `type Output` — associated types fragment the trait
   object. (This is the same constraint that produced the existing object-safe
   `TurnDrivenRuntime` / `LoopSession`.)
3. **Core trait + `Ext` trait.** Keep the object-safe trait minimal; put
   ergonomic generic helpers in a blanket-impl'd `…Ext: …` trait
   (`impl<T: Tool + ?Sized> ToolExt for T {}`), never called through `dyn`.
4. **Orphan rule dictates adapter placement.** Every cross-crate adapter needs a
   local newtype in the *assembling* crate (`McpToolAdapter`, `WorkflowTool`,
   `AgentNode`). This is why the MCP→`Tool` adapter lives in `agentflow-skills`,
   and it generalizes to all adapters — adapters never pollute contract crates.
5. **Type-state + newtypes eliminate whole bug classes** (per the project's own
   "encode invariants in types, don't `panic`" guidance):
   - `Session<Active>` / `Session<Finished>` — calling `next_turn` on a finished
     session becomes a *compile* error (replaces the `SessionFinished` guard).
   - `Seq` newtype + a `SeqAllocator::stamp(event)` that couples allocation and
     dispatch in one critical section — closes the seq-vs-write ordering race.
   - `Validated<ModelId>` constructible only after a successful build — closes
     the chat-REPL "stale `cur_model` after failed switch" bug.
   - `ByteSafeStr` / only exposing `.chars().take(n)` — closes the UTF-8
     byte-slice panic at the API level.
6. **Evolution & errors.** `#[non_exhaustive]` on every contract enum (adding a
   variant is non-breaking — required for the beta-stable `HarnessEvent` wire);
   sealed traits for internal-only protocols; each contract crate owns a
   `thiserror` boundary error enum that impls map into via `From`.
7. **Feature flags = compile-time low coupling.** Contract crates are
   feature-less (always present); optional capabilities (`rag`, `mcp`) are
   gated by consumers; features must be additive. Combined with the orphan rule,
   a capability that is not compiled has no `impl Capability` to wire — zero
   binary, zero runtime cost.

## 7. Eight dependency laws (enforced by `xtask check-arch`)

1. Contract crates depend only downward to `value`; never on any impl.
2. A tool crate depends on the `tool` contract only; not on runtimes or other tools.
3. A capability depends on contracts + the tools it wraps; `impl Capability::lower`; never on a runtime.
4. **Runtimes never depend on each other**; only on contracts; capabilities/tools are injected by surfaces.
5. **IR ≠ executor**: `Flow` in `graph`, scheduler in `core`.
6. The executor depends on `graph`/`value`/`async-util`; never on `agents`/`harness`.
7. Reliability primitives live once in `async-util`; `core` and `agents` reuse them.
8. Surfaces are the only tier allowed to import the whole graph.

## 8. Four execution paradigms on this architecture

| Paradigm | Plan author | Executor | Crates touched (contracts + self) |
|---|---|---|---|
| Static DAG | human (code/YAML) → `Flow` | `core` | graph → core |
| Native loop | `agents` (implicit per-step) | `agents` | agent-spi, graph, tool |
| Harness | as above + governance | `agents` + `harness` shell | harness → agent-spi (**not** agents) |
| **Dynamic workflow** | `agents` **generates** `Flow` | `core` executes; nodes may be `AgentNode`; `harness` governs | agents→graph build, core→graph execute, harness→agent-spi govern — **all meet only via contracts** |

The last row is the payoff: once `graph` and `agent-spi` are the waist, dynamic
workflow requires **no new sideways dependency**.

## 9. RAG repositioning

RAG is on the *capability* axis, not a top-level mode. `agentflow-rag` becomes a
`KnowledgeBackend` implementation behind a Skill's `knowledge:` declaration with
tiered progressive disclosure (frontmatter → bundled files via grep/read → RAG
vector retrieval → structured query). RAG fires only when bundled-file
navigation is insufficient (large / dynamic / multi-tenant corpora). The eval
harness is retained — it is the only quality gate for the cases RAG is still
for. The user-facing `rag search/index` CLI demotes to ops subcommands.

**Status (P-A4.1):** the `KnowledgeBackend` SPI lives in `agentflow-store-spi`
(alongside `MemoryStore`); `agentflow-rag` implements it as `Bm25KnowledgeBackend`
(bundled-files tier) + `VectorStoreKnowledgeBackend` (vector tier) and exposes
the `rag_search` `Tool` (`RagSearchTool`).

**Status (P-A4.2):** a Skill's `[[knowledge]]` entries carry a `backend` field
(`files` default — inline into the persona; `rag` — index the bundled files and
expose the `rag_search` tool). `SkillBuilder` routes each entry independently.

**Status (P-A4.1b):** the user-facing `rag search` / `rag index` /
`rag collections` CLI is demoted under an `ops` group (`agentflow rag ops
<cmd>`); `rag eval` stays top-level (it is the quality gate). The agent-facing
retrieval path is the `rag_search` tool a Skill exposes.

## 10. Migration — strangler fig (no rewrite)

Grow the new kernel inside the existing workspace; re-point crates one at a time
behind `pub use` re-exports; keep `cargo test` + `clippy -D warnings` green per
PR. Execution is tracked as track **P-A** in `TODOs.md`:

- **P-A0 — Guardrails.** Land this RFC; add `xtask check-arch` with the eight
  laws and an allowlist of current violations to burn down.
- **P-A1 — Contract kernel.** Create `agent-spi`, `store-spi`, split `graph`
  out of `core`, extract `async-util`; re-export from old paths. Prove the
  design with a `dynamic_workflow` vertical-slice spike (`value → graph →
  agent-spi → toy runtime → agent emits Flow → core executes`).
- **P-A2 — Runtime decoupling.** Re-point `harness` from `agents` to
  `agent-spi` (injection at surfaces); let harness govern a `Flow` run; extract
  `worker-proto` so `worker ⊥ server`; extract shared assembly so `server ⊥ cli`.
- **P-A3 — Reliability + type hardening.** Consolidate retry/timeout/cancel into
  `async-util`; introduce `Session<S>` type-state, `Seq` newtype, `ByteSafeStr`,
  `Validated<ModelId>` (closes the known bug classes).
- **P-A4 — Dynamic workflow + RAG repositioning.** `rag` → `KnowledgeBackend`;
  `PlanExecuteAgent` emits a real `Flow`; productionize the spike;
  `Capability::lower` wiring in `skills`.

P-A0+P-A1 are low-risk and do "now"; P-A2/P-A3 are gated behind thickened tests;
P-A4 is feature work sitting on the cleaned dependencies.

## 11. Out of scope / untouched

`core` IR semantics, `tools` built-ins, the 6 `llm` providers, `mcp`, `db`,
`ui`, and all external contracts (CLI / workflow YAML / `SKILL.md` / HTTP
`/v1/*`). No crate merges/splits beyond the kernel listed in §4. This is a pure
internal dependency re-arrangement.

## 12. Risks

- **P-A3 touches the hot loop** (`react/agent.rs`) — gate behind expanded tests;
  reuse the existing "faithful extraction" discipline.
- **Re-export churn** — mechanical; mitigate with one-crate-per-PR and
  `xtask check-arch` regression-proofing.
- **Kernel over-fragmentation** — capped at six single-responsibility crates by
  the distinct-dependent-set rule; no further splitting without that test.

## 13. Refinements (2026-06-20 architecture-lens evaluation)

`docs/ARCHITECTURE_EVALUATION_2026-06-20.md` validated this RFC against the
src-confirmed dependency graph (per-edge symbol analysis, not just crate-level
deps). Verdict: **direction confirmed, adopt as-is**, with these deltas folded in:

- **R1 — `value` is promoted, not deferred.** §4 marked `agentflow-value` "defer
  (re-export)"; it is in fact a hard prerequisite of `graph` (which depends on
  `FlowValue`) and the most widely-imported leaf. Extract it **first** in P-A1.
- **R2 — the `agents→core` split is risk-free.** Symbol analysis shows `agents`
  imports only IR symbols from `core` (`AsyncNode` / `Flow` / `FlowValue` /
  `AsyncNodeInputs` / `AgentFlowError`) and **zero executor symbols** — so the §5
  `graph` split resolves the edge with no residual coupling.
- **R3 — `agentflow-nodes` needs an explicit decomposition decision** (the one
  genuine crate-division gap, tracked as P-A0.5). `nodes` straddles tool /
  capability / runtime tiers; split tool-tier nodes
  (`template`/`file`/`http`/`batch`/`conditional`/`arxiv`/`markmap` →
  `graph`+`tool` only) away from capability-backed nodes
  (`llm`/`asr`/`tts`/`image*`/`rag`/`mcp`). **Decided** in
  `docs/RFC_NODES_DECOMPOSITION.md`: split into `agentflow-nodes` (tool-tier) +
  a new `agentflow-nodes-ai` adapter crate; lands in P-A4 after the `graph` split.
- **R4 — `llm→core` is already gone** (removed Q3.6.1); §1 seam #2 and the §4
  `llm` row overstate `llm`'s coupling. `llm` has no internal deps.
- **R5 — track the full target-state edge map**, not just the 4 gate-enforced
  runtime/surface edges. Seven latent capability/tool violations (evaluation §2
  rows 5–11) must be repointed as the kernel lands; notably `harness` carries
  **5** impl edges, of which the allowlist tracks only `harness→agents`.
- **R6 — fold three "thin reason, fat dep" edges into existing kernel crates**
  rather than minting micro-crates: `harness→llm` (tokenizer only) and
  `memory→rag` (`EmbeddingProvider` only) → `store-spi`/`value`; `mcp→tracing`
  (traceparent ambient only) → `agent-spi`/`value`. Keeps the kernel at six.
