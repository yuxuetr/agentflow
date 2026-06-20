# RFC Addendum: `agentflow-nodes` Decomposition

- Status: **Decided** — resolves the one open crate-division question from
  `docs/ARCHITECTURE_EVALUATION_2026-06-20.md` (refinement **R3**).
- Parent: `docs/RFC_CRATE_ARCHITECTURE.md` (§3 tiering, §6.4 orphan rule).
- Tracking: `TODOs.md` §P-A0.5; must be settled before P-A4 (worker node
  coverage + dynamic-workflow node reuse both depend on it).
- Scope: internal crate split only. No change to workflow YAML node `type:`
  names, the `AsyncNode` contract, or any public API.

## Problem

`agentflow-nodes` is the workspace's only crate whose dependency-set does not fit
a single RFC tier. The evaluation flagged it a **fat straddler**: filed under the
Tool tier, it depends on a runtime (`core`) *and* two capabilities (`llm`, `rag`)
*and* another tool (`mcp`). That makes the latent law-2 violations
`nodes→{llm,rag,mcp,core}` (architecture evaluation §2 rows 7–8) — and, more
concretely, it forces any consumer that only wants tool-tier nodes (notably
`agentflow-worker`) to compile `llm` + `rag` + `mcp` it will never execute.

## Measured partition (per-file capability imports)

The split is unambiguous — each node file either imports a capability or it does
not:

| Bucket | Node files | Cross-crate imports |
|---|---|---|
| **Tool-tier** (7) | `template`, `file`, `http`, `batch`, `conditional`, `arxiv`, `markmap` | `core` IR only (→ `graph`) |
| **Capability-backed** (10) | `llm`, `asr`, `tts`, `text_to_image`, `image_to_image`, `image_understand`, `image_edit` | `→ agentflow-llm` |
| | `rag` | `→ agentflow-rag` |
| | `mcp` | `→ agentflow-mcp` |

## Decision

**Split into two crates** (RFC §3 Tool tier, kept honest):

1. **`agentflow-nodes`** (unchanged name) — tool-tier `AsyncNode` library:
   `template` / `file` / `http` / `batch` / `conditional` / `arxiv` / `markmap`.
   Dependencies: `graph` + `tool` only (plus leaf utilities like the HTTP client
   and Tera). This is what pure-DAG consumers and `agentflow-worker` depend on.
2. **`agentflow-nodes-ai`** (new) — capability-backed `AsyncNode` adapters:
   `llm` / `asr` / `tts` / `image*` / `rag` / `mcp`. Dependencies: `graph` +
   the capabilities (`llm`, `rag`, `mcp`). This is an *adapter* crate (RFC §6.4:
   adapters live in the assembling crate, never in a contract crate), depended on
   by the full surfaces (`cli`, `server`).

### Why a split and not feature-gating

Feature-gating the capability nodes inside one crate does **not** pay the edge
down: `check-arch` reads the `[dependencies]` table, where an `optional = true`
dep is still a declared edge. The latent `nodes→{llm,rag,mcp}` edges would
persist in the graph regardless of features. Only moving those nodes into a
separate crate removes the edge from the tool-tier crate — which is the whole
point of the law. (Per-feature flags remain useful *within* `agentflow-nodes-ai`
to keep e.g. `rag`/`mcp` optional there.)

### Why one adapter crate and not distributing nodes to each capability

Moving the `llm` node into `agentflow-llm`, the `rag` node into `agentflow-rag`,
etc., would also satisfy the law (each capability would gain a `graph` dep behind
a `node` feature). Rejected because it **fragments the node factory/registry**
(`factory_traits.rs`) across crates: the config-first loader builds every node
type from one registry, and scattering the adapters complicates that single
assembly point for no dependency-graph benefit. One `agentflow-nodes-ai` crate
keeps the adapters + their factory wiring cohesive.

## Resulting edges (vs the latent map)

| Latent edge (eval §2) | After split |
|---|---|
| `nodes → core` | `agentflow-nodes → graph` (IR-only; law-clean) |
| `nodes → llm` | moves to `agentflow-nodes-ai → llm` (allowed: adapter crate) |
| `nodes → rag` | moves to `agentflow-nodes-ai → rag` |
| `nodes → mcp` | moves to `agentflow-nodes-ai → mcp` |

`worker → nodes` then carries **zero** capability weight, unblocking the P2.8 /
P-A4 worker node-coverage work. The four `nodes→*` rows leave `ARCH_LATENT_EDGES`
(pruned by the gate's staleness guard) as the split lands.

## Sequencing

Lands in **P-A4** (after the `graph` split in P-A1.3, since both crates depend on
`graph`). Mechanical: move 10 files + their factory registrations into the new
crate, re-export from `agentflow-nodes` under a deprecated `pub use` shim for one
release so no consumer breaks, then repoint `worker` to default (tool-tier) and
`cli`/`server` to both crates. Keep `cargo test` + `check-arch` green per step.
