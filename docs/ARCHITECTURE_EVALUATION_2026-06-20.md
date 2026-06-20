# AgentFlow Architecture-Lens Evaluation — 2026-06-20

- Status: **Complete** — validates and refines `docs/RFC_CRATE_ARCHITECTURE.md`.
- Scope: all 16 workspace crates (15 Rust + 1 TS SPA), **architecture / dependency
  lens only** — complements the security/code-quality lens of `docs/audit/`
  (2026-05-24) rather than repeating it.
- Question driving this pass: *can the four execution paradigms (static DAG /
  native loop / harness / **dynamic workflow**) coexist as one coherent stack,
  and is the RFC's contract-kernel crate division the right way to get there?*
- Method: ground-truth internal dependency graph parsed from every
  `Cargo.toml` `[dependencies]` (dev-deps excluded — test-only, don't shape the
  shipped graph), then **per-edge symbol analysis** (`grep "use agentflow_X::"`)
  to learn *why* each edge exists, not just *that* it exists. No guesswork.

## TL;DR

The RFC is **fundamentally sound — direction confirmed, adopt as-is** with six
refinements (R1–R6 below). The four paradigms already coexist; the only thing
keeping them as semi-integrated parallel code paths is that **runtimes are fused
to concrete capability/tool impls instead of contracts**, and **the DAG IR is
fused with the executor**. The narrow-waist contract kernel fixes both. Two
findings strengthen the case and one exposes a genuine gap the RFC under-specifies:

- **Confirming (de-risks the plan):** `agents → core` imports **only IR symbols**
  (`AsyncNode` / `Flow` / `FlowValue` / `AsyncNodeInputs` / `AgentFlowError`) and
  **zero executor symbols** — so splitting `graph` out of `core` (P-A1.3) fully
  resolves the `agents→core` violation with *no residual coupling*. `core` itself
  depends on **nothing internal** — the executor never reaches up. The cleanest
  possible starting point.
- **Strengthening:** `llm` already has **no internal deps** (the `llm→core` edge
  the RFC still implies was removed in Q3.6.1). The RFC text should drop it.
- **Gap:** `agentflow-nodes` is a **fat straddler** — a tool-tier `AsyncNode`
  library that also *consumes capabilities* (`llm`, `rag`) and a runtime (`core`).
  The RFC files `nodes` under "Tools" but its real dependency-set doesn't fit any
  single tier. This needs an explicit decomposition decision (R3).

The `xtask check-arch` gate currently enforces **2 of the 8 laws** (runtime- and
surface-isolation) and tracks **4 allow-listed edges**. The remaining **7 latent
violations** (against laws not yet activatable because the kernel crates don't
exist) are enumerated below so each kernel-landing PR has its burndown list ready.

## 1. Ground-truth dependency graph (src-confirmed, `[dependencies]` only)

| Crate | RFC tier (claimed) | Internal deps (real) | Edge reason (imported symbols) |
|---|---|---|---|
| `agentflow-core` | Runtime (executor) **+ holds IR + `FlowValue`** | — | (depends on nothing internal) |
| `agentflow-llm` | Capability | — | (no internal deps; `llm→core` removed Q3.6.1) |
| `agentflow-tools` | Tool contract + builtins | — | (no internal deps) |
| `agentflow-rag` | Capability + tool | — | (no internal deps) |
| `agentflow-db` | Ops / persistence leaf | — | (no internal deps) |
| `agentflow-tracing` | Ops | `core` | `events::{EventListener, WorkflowEvent}` |
| `agentflow-mcp` | Tool | `tracing` | `context::{current_traceparent, scope}` — **traceparent ambient only** |
| `agentflow-memory` | Capability | `rag` | `embeddings::EmbeddingProvider` — **SemanticMemory only** |
| `agentflow-nodes` | Tool | `core, llm, mcp, rag, tools` | `core`=IR; `llm`=`AgentFlow/Asr/Tts/Image`; `rag`=retrieval nodes |
| `agentflow-agents` | Runtime (loop) | `core, llm, mcp, memory, tools` | `core`=**IR-only**; `llm`/`memory`/`tools`=concrete impls |
| `agentflow-skills` | Capability | `agents, mcp, memory, rag, tools` | `agents`=**runtime** (builds runnable agent) |
| `agentflow-harness` | Runtime (shell) | `agents, llm, memory, tools, tracing` | `llm`=**tokenizer only**; `tracing`=redaction/storage/types |
| `agentflow-server` | Surface | `agents, cli, core, db, harness, llm, memory, skills, tools, tracing` | surface assembly (+ `cli` edge — allow-listed) |
| `agentflow-worker` | Surface | `agents, core, llm, memory, nodes, server, tools` | surface (+ `server` edge — allow-listed) |
| `agentflow-cli` | Surface | `agents, core, db, harness, llm, mcp, memory, nodes, rag, server, skills, tools, tracing` | top assembly (imports the world — allowed) |
| `agentflow-ui` | Surface (SPA) | — (HTTP client of `/v1/*`) | n/a |

## 2. The eight laws vs reality — full violation map

`check-arch` enforces laws 4/6 (runtime-isolation) and the surface-isolation
corollary today. The table marks **[gate]** for the 4 edges it already tracks and
**[latent]** for the 7 it can't yet, because the target-tier laws (1/2/3/7) only
become checkable once the kernel crates exist. **All 11 are real target-state debt.**

| # | Edge | Breaks law | Status | Resolution |
|---|---|---|---|---|
| 1 | `agents → core` | 4/6 runtime-isolation | **[gate]** allow-listed | P-A1.3 `graph` split — edge becomes `agents→graph` (IR-only ✔) |
| 2 | `harness → agents` | 4/6 runtime-isolation | **[gate]** allow-listed | P-A2.1 repoint to `agent-spi`; runtime injected by surfaces |
| 3 | `worker → server` | surface-isolation | **[gate]** allow-listed | P-A2.3 extract `worker-proto` |
| 4 | `server → cli` | surface-isolation | **[gate]** allow-listed | P-A2.4 extract shared assembly crate |
| 5 | `agents → {llm, mcp, memory, tools}` | 4 (runtime fused to impls) | **[latent]** | inject via `agent-spi`/`store-spi`/`tool` at surfaces |
| 6 | `harness → {llm, memory, tools, tracing}` | 4 (runtime fused to impls) | **[latent]** | as above — **allowlist under-counts harness debt by 4 edges** |
| 7 | `nodes → {llm, rag}` | 2 (tool crate → capabilities) | **[latent]** | R3 — decompose `nodes` |
| 8 | `nodes → core` | 2 (tool crate → runtime) | **[latent]** | IR-only → `nodes→graph` after P-A1.3 |
| 9 | `skills → agents` | 3 (capability → runtime) | **[latent]** | P-A4.3 `Capability::lower`; surface wires the runtime |
| 10 | `memory → rag` | 3 (capability → capability) | **[latent]** | route via `store-spi` embeddings contract (R6) |
| 11 | `mcp → tracing` | 2 (tool → ops) | **[latent]** | trace-context contract leaf (R6) |
| — | `tracing → core` | (ops → runtime) | **[latent]** | becomes `tracing→agent-spi`+`value` once they exist |

**Key insight:** edges 5–6 are the structural heart of "four parallel paths."
`agents` and `harness` are the two live runtimes; today each hard-wires the
concrete `llm`/`memory`/`tools` it drives. Once those become `agent-spi` /
`store-spi` / `tool` contract edges with injection at the surface, the two
runtimes stop being bespoke stacks and start being interchangeable drivers over
one waist — which is exactly what makes dynamic workflow "free."

## 3. Per-crate verdict (architecture lens)

- **`core`** — Soundest crate in the tree: zero internal deps, executor never
  reaches up. Its *only* architectural debt is law 5 (IR ≠ executor): `value.rs`,
  `async_node.rs`, `node.rs`, `flow.rs`, `expr.rs` (IR) live beside `scheduler.rs`,
  `retry*.rs`, `timeout.rs`, `checkpoint.rs`, `resource_*.rs` (executor). Split is
  mechanical; no consumer imports executor symbols it shouldn't.
- **`llm`** — Clean leaf. No internal deps. Keep as a capability; its tokenizer
  sub-module is what `harness`/`agents` actually reach for (R6).
- **`tools`** — Already the `Tool` contract + builtins fused. RFC §4 carves the
  contract into `agentflow-tool` and moves builtins to `agentflow-tools-builtin`;
  low urgency (no bad inbound edges), do it when convenient.
- **`rag`** — Clean leaf today. Repositions to a `KnowledgeBackend` capability
  (P-A4.1); keep the eval harness as the quality gate.
- **`memory`** — Capability with one capability→capability edge (`→rag` for
  `EmbeddingProvider`). Minor; fold behind `store-spi` (R6).
- **`tracing`** — Ops crate; `→core` for the workflow event types. Becomes
  `→agent-spi` + `→value`. The `redaction` module is (correctly) the thing
  `harness` reuses — keep it reusable.
- **`mcp`** — Tool crate; only bad edge is `→tracing` for the traceparent ambient
  (R6). Otherwise a clean tool provider.
- **`nodes`** — **The one real crate-division problem (R3).** Straddles tool /
  capability / runtime tiers. Largest cohesion debt in the workspace.
- **`agents`** — Runtime. `→core` is IR-only (resolves cleanly via `graph`); the
  `→{llm,mcp,memory,tools}` edges are the impl-fusion to undo via contracts.
- **`skills`** — Capability that depends on the `agents` runtime (inversion).
  The `Capability::lower` work (P-A4.3) flips this the right way up.
- **`harness`** — Runtime shell carrying **5 impl edges**, only 1 tracked. After
  P-A2.1, four remain (`llm` tokenizer, `memory`, `tools`, `tracing`). The
  governance shell should sit on `agent-spi` + the reusable `redaction`/token
  contracts, nothing more.
- **`db` / `server` / `worker` / `cli` / `ui`** — Surfaces (+ db leaf). Allowed to
  assemble broadly; the two cross-surface edges (3, 4) are the only debt, already
  tracked. `ui` is a pure `/v1/*` HTTP client — correctly outside the Rust graph.

## 4. RFC validation + six refinements

The RFC's core decisions are **confirmed**: refactor-in-place (not rewrite);
narrow-waist contract kernel; Capability-vs-Tool as the two load-bearing traits;
IR ≠ executor; dyn-at-seams / generic-inside; `#[non_exhaustive]` contract enums.
Refinements (deltas, not reversals):

- **R1 — Promote `value`; do not defer it.** RFC §4 marks `agentflow-value`
  "defer (re-export)". But `graph` *depends on* `FlowValue`, and `FlowValue`
  (in `core/src/value.rs`) is imported by the widest set of crates. Extract
  `value` **first** in P-A1 as the cheapest, highest-leverage cut and a hard
  prerequisite of the `graph` split — not a later nicety.
- **R2 — Record that `agents→core` is IR-only.** Add to RFC §5 / the P-A1.3 task:
  symbol analysis shows zero executor imports, so the split carries no risk of a
  hidden executor dependency surviving. De-risks the highest-churn step.
- **R3 — Give `nodes` an explicit decomposition decision (new RFC §9a / P-A task).**
  Options: (a) **split** `nodes` into `nodes-core` (tool-tier: `template`/`file`/
  `http`/`batch`/`conditional`/`while` — `graph` + `tool` only) and capability-
  backed nodes (`llm`/`asr`/`tts`/`image*`/`rag`/`mcp`) that move beside their
  capability or become thin adapters in a surface/assembly crate; (b) **re-tier**
  `nodes` as an assembly crate (not a leaf tool crate) and exempt it like a
  surface. Recommended: (a) — it's the only choice that keeps the tool tier honest
  and lets `worker` depend on tool-tier nodes without dragging `llm`/`rag` in.
- **R4 — Drop the stale `llm→core` claim** from RFC §1 (seam #2 framing) and §4
  (the `llm` dependent row). Already removed in Q3.6.1.
- **R5 — Ship the full target-state edge map, not just the 4 gate edges.** The
  `check-arch` allowlist tracks 4 runtime/surface edges; the kernel migration must
  also repoint ~7 capability/tool edges (table §2 rows 5–11). Add these as a
  burndown checklist (P-A0.4) so activating each new law (a "one-line change" per
  the RFC) lands with its edge list already paid down — and so the `harness`
  4-edge under-count (row 6) is visible.
- **R6 — Decide the three "thin reason, fat dep" edges deliberately.** `harness→llm`
  (tokenizer only), `mcp→tracing` (traceparent only), `memory→rag` (embeddings
  only) each pull a whole crate for one small surface. Don't mint three micro-crates
  — **fold** them into kernel crates that already exist in the plan: token-count →
  a util in `value` or `store-spi`; trace-context ambient → `agent-spi` (or `value`);
  `EmbeddingProvider` → `store-spi`. This keeps the kernel at six crates (RFC §12
  cap) while killing three latent violations.

## 5. Crate-count sanity check (no over-splitting)

Applying the RFC's own rule — *a crate exists iff its dependent-set is distinct* —
to the proposed end state: `value` (universal leaf), `tool`, `graph`,
`store-spi`, `agent-spi`, `async-util` each have genuinely distinct dependent-sets
(confirmed against §1). `store-spi` cannot fold into `agent-spi` without forcing
`rag`/`memory` to depend on `AgentRuntime`; `value` cannot fold upward. The
**six** kernel crates are warranted. R3 adds **one** decomposition inside the
existing `nodes` crate — net new published crates from this evaluation: the six
kernel + (`worker-proto`, shared-assembly from P-A2) + `nodes-core` split. No
god-crates, no ceremony crates.

## 6. Recommendations → ROADMAP / TODOs

1. Adopt the RFC as the architecture of record (it already is). Fold R1–R6 into
   `docs/RFC_CRATE_ARCHITECTURE.md` and the `P-A` track in `TODOs.md`.
2. Refresh `RoadMap.md` (stale 2026-05-14, pre-kernel) to name the **four**
   paradigms and the contract-kernel direction, demoting "two first-class paths."
3. Keep ordering: **P-A runs after the Q security/correctness waves** — an
   architecture refactor must never preempt production-blocking fixes.
4. Sequence within P-A1: `value` (R1) → `graph` (R2) → `agent-spi`/`store-spi`
   (absorbing R6 contracts) → `async-util` → dynamic-workflow spike.
5. Land the `nodes` decomposition decision (R3) as a short RFC addendum before
   P-A4 (worker node-coverage and dynamic-workflow node reuse both depend on it).
