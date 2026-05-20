# Roadmap v2 — Post-v1.0 GA Direction

This document is the **single source of truth for "what's after
v1.0 GA"**. Before P10.19.3 the same signals lived in three
places: `RoadMap.md` "Later Tracks", `docs/archive/
PROJECT_EVALUATION_2026-05-19.md` §7.3, and the `v1.x` entries
in `TODOs.md`. Consolidating them here makes the post-v1 picture
auditable in one read.

## Status

- **Not binding.** Items here are directional. They graduate to
  `TODOs.md` only when they become concrete (clear acceptance
  criteria + owner). This doc is the staging ground.
- **Not exhaustive.** New ideas land here when they're worth
  remembering but not worth executing today.
- **Living document.** Edit freely. Remove items that landed,
  rename items as the shape sharpens, add items as they
  surface.

## v1 → v2 inflection

v1.0 GA closes when the P10.0.x release-engineering checklist
runs clean (production deployment dress-rehearsal, `cargo
publish --dry-run`, tag, image push, fresh-VM doctor smoke).
After GA the maintenance bar tightens:

- **Wire shapes pinned by fixture tests don't change.** Anything
  in `docs/STABILITY.md` Stable or Beta tier is now load-bearing
  for downstream users; bumping requires a major version or an
  explicit migration window.
- **Operator UX > new capability surface.** The v1 → v2 work
  emphasises hardening, observability, and ergonomics rather
  than new product surfaces.
- **"Operator dashboard" stays out.** Per P10.17.1, the Web UI
  is a debugger / run console. Operator-dashboard features
  (cost / retry-rate / worker utilization / policy summaries)
  are explicitly out of scope; Prometheus + Grafana + BI tools
  cover those better. See
  [`docs/WEB_UI.md` § Product positioning](WEB_UI.md#product-positioning).

## Themes

Each theme below collects related items. Item-level references
point at the canonical source so the inventory is auditable
(`TODOs.md::P10.X` for tracked items, `RoadMap.md` for the
"Later Tracks" prose, etc.).

### A. LLM provider & model layer

Motivation: the 9-provider live nightly is green (N9) and the
modality dispatcher is unified (P-LLM). The remaining work is
hardening + finer-grained vendor compatibility.

- **Promote DashScope / DeepSeek / MiniMax to dedicated provider
  modules** (`TODOs.md::P10.3.2`). Today they share
  `OpenAIProvider` via `create_provider`. Move only when a
  vendor introduces a wire-format divergence the shared adapter
  can't cleanly handle. Estimate: ~300-500 LoC per vendor.
- **Provider-specific tokenizers** (`P10.3.3`). The current
  `PricingTable` cost tracking + `RuntimeLimits` token budgets
  use char-count heuristics. Wire each provider to its real
  tokenizer (tiktoken for OpenAI, etc.) for accurate budget
  enforcement.
- **Auto-rotate live nightly default models on 404** (`P10.3.4`).
  Vendor deprecations cause periodic CI churn (claude-3-5-haiku
  → claude-haiku-4-5; gemini-1.5-flash → 2.5-flash, etc.). A
  `cargo xtask refresh-live-models` would ping each provider's
  models-list endpoint + rotate the workflow env block. Tracked
  also under `P10.18.1`.

### B. Memory & RAG layer

Motivation: P4 landed the 4-layer memory design + RAG eval
harness. Production-grade encryption + cross-session linking +
richer eval baselines are next.

- **Encryption-at-rest** (`P10.7.2`). `EncryptedPreferenceStore`
  trait stub is in place. Pick a KMS strategy (age / sops /
  cloud KMS via envelope encryption?) and ship a real impl.
- **Cross-session memory linking** (`P10.7.3`). The 4-layer
  design separates Session / Semantic / Preference / Entity
  facts cleanly. A "memory graph" linking entities across
  sessions is a v2 design conversation.
- **Pluggable retriever expansion** (`P10.6.1` landed BM25 +
  dense + hybrid). Future: external retrievers (Qdrant
  production, Elasticsearch, custom domain-specific). Easy via
  the `Retriever` trait — trigger when a real use case
  surfaces.

### C. Server / platform productization

Motivation: P2 + P5 landed the server skeleton + worker control
plane. v1.x hardening focuses on per-tenant operations.

- **Per-run retention override** (`P10.14.1`). Today retention
  is per-tenant + per-profile. P2.2 left per-run override as
  deferred. `POST /v1/runs` body could carry
  `retention_overrides: { events_days, artifacts_days }` for
  keeping critical runs longer than the global sweep.
- **Grafana dashboard templates** (`P10.14.2`). Server emits
  Prometheus metrics; checked-in Grafana JSON would let
  operators import in 1 click. (NB: this is operator-dashboard
  territory but lives in `agentflow-server` deliverables, not
  the UI — consistent with the P10.17.1 positioning.)
- **Real backup/restore implementation** (`P10.15.1`). Today:
  docs + `agentflow doctor --backup-check` probes; production
  uses `pg_dump` + filesystem snapshot. An `agentflow backup
  --output <path>` orchestrator would close the loop.
- **Read-replica support** (`P10.15.2`). All repos write
  through the primary. A `--database-read-url` option routing
  `list_*` / `get_*` reads to a replica would scale read-heavy
  gateways.

### D. Web UI

Motivation: positioning is now firmly **debugger / run
console**. v1.x work stays inside that boundary. Operator
dashboard is explicitly NOT v2 scope.

In scope (each is a small additive feature inside the
debugger boundary):

- **Harness session replay UI** — visual analogue of
  `agentflow harness replay --speed 2x` (P10.10.2 landed the
  CLI). Stretch.
- **Trace compare polish** — better diffs, more event-type
  coverage.
- **Preference UI wiring follow-through** — extend P10.17.2's
  proof-of-pattern (run-console tenant) to the other 6
  syncable keys; the 3-line replication pattern lives in
  `agentflow-ui/src/main.tsx::RunConsole`.

Architectural decisions already landed:

- **`agentflow-viz` deleted** (`P10.13.1`, closed). Honest
  audit revealed the "DAG visualisation" was a button grid of
  node status badges + the raw Mermaid markdown text in a
  `<pre>` block — no SVG, no spatial layout, no edges. The
  data plumbing through `/v1/runs/{id}/graph` was disproportionate
  to the rendering value. Crate + endpoint + UI consumer all
  removed. Workflow DAG visualisation **could** be done well
  (e.g. mount mermaid.js in the UI to render the text as SVG)
  but is deferred to a future RFC. Agent-native execution
  visualisation (ReAct loop / multi-agent topology / Harness
  session tree) is its own design problem — today's surface is
  `agentflow harness replay --speed 2x` per-event text timeline.

Explicitly NOT v2:

- Cost / billing aggregation, retry-rate trends, worker
  utilization, policy-decision summary tabs. The
  [`WEB_UI.md` in/out table](WEB_UI.md#product-positioning) is
  the canonical list.

### E. Distributed execution & worker hardening

Motivation: `agentflow-worker` exists with gRPC `WorkerProtocol`
+ admission policy + PSK auth. v1.x adds identity + scheduling
intelligence.

- **Signed-JWT identity** (`P10.16.1`). Today PSK-only via
  `WorkerCredential`. JWT is documented as the next iteration
  when the broader auth track ships issuer / audience / key
  rotation primitives.
- **Worker pool admission heuristics** (`P10.16.2`). Static
  `max_workers` + `max_concurrent_tasks_per_worker` today.
  Future: capacity-aware load balancing, locality hints
  (`run_dir` co-location), per-worker capability advertising
  (which node types each worker can run).
- **Worker-executable node payload expansion** — `P2.8`
  expanded to `template/file`; future iterations should add
  `llm`, `http`, `mcp`, `agent` so worker mode is useful for
  real workloads.
- **Distributed scheduling validation** — large DAG scheduling
  with mixed local + worker-executed nodes; failure-domain
  tests.

### F. Harness Agent Mode (H6 items)

Motivation: H0–H5 closed. H6 is the open-ended track of
"advanced compatibility" features — promoted individually only
when concrete demand surfaces (`P10.10.1`).

Candidate items (each needs its own RFC before promotion):

- **Slash-command ecosystem expansion** — beyond the current
  closed command set.
- **TUI product shell** — separate from CLI run; an opinionated
  long-lived agent UI.
- **OpenHarness-style config import** — interop with the
  external `harness` config conventions.
- **Plugin compatibility adapters** — bridges for existing
  third-party agent frameworks.
- **Provider subscription bridge** — bring-your-own-subscription
  flows.

Don't pull en bloc. Each is its own design conversation.

Already-closed Harness session replay CLI is P10.10.2 (landed).

### G. Plugin runtime expansion

Motivation: subprocess JSON-RPC is the stable v1 runtime. v2's
question is what supplements it.

- **WASM plugin runtime evaluation** (`P10.19.1`). Subprocess
  is mature. WASM is the natural v2 plugin runtime — sandbox
  is free, distribution is single-file, startup is faster.
  Action: write a 1-pager comparing wasmtime / wasmer / extism
  against the current subprocess `Plugin` trait surface.
  Decide whether to invest pre-v1.0 GA or push to v2.0.
- **Avoid native `dlopen`** — unless a concrete ABI strategy
  emerges. The Non-Goals stance from `RoadMap.md` carries
  forward.

### H. Performance & regression detection

Motivation: P7 + `bench-gate` xtask exist; baseline workspace
coverage is uneven.

- **Hot-path benchmark suite** (`P10.1.1`). Criterion benches
  exist (`benches/scheduler.rs`); compare against `bench-gate`
  baseline and watch for 1.10× regressions.
- **Per-node latency benches** (`P10.2.1`). Today only
  scheduler mechanics are benched. Per-node hot-path benches
  would catch e.g. template-render regressions.
- **Workspace-wide perf regression detection** (`P10.19.2`).
  `bench-gate` covers criterion; extending to `cargo test
  --workspace` wall-clock per crate would catch test-suite
  bloat early. Gate on 1.5× regressions.

### I. Ops & developer tooling

- **`cargo xtask refresh-live-models`** (`P10.18.1`). See
  Theme A above. Reduces manual chore toil after vendor
  deprecations.
- **`cargo xtask check-changelog`** (`P10.18.2`, landed) — now
  available as a manual gate; promotion to `quality.yml`
  release-gate is the next step once the heuristic has run
  against real PRs.
- **Operational deployment profiles** — local dev, single-node
  server, distributed worker cluster (from `RoadMap.md::
  Operations`).
- **E2E smoke tests covering CLI + server + worker + tracing +
  Web UI paths** (from `RoadMap.md::Operations`). The UI
  e2e slice (P10.17.4) is the wedge.

### J. Documentation & contributor experience

- **CHANGELOG hygiene** — the new `cargo xtask check-changelog`
  enforces it locally; CI promotion is the natural follow-up.
- **Centralised release notes** — `docs/RELEASE_NOTES_*.md`
  per version (pattern set by `v1.0.0-rc.1.md` DRAFT).
- **Stretch json-envelope migrations** — periodic audit per the
  P10.11.3 pattern; today's audit landed exactly one holdout
  fix. Recheck after every new CLI subcommand added.

## Non-goals for v2

These stay out unless a strong concrete signal lands. The
v1-era non-goals (`RoadMap.md::Non-Goals For V1`) carry
forward; v2 adds explicitly:

- **Web UI as operator dashboard** — out per P10.17.1. Use
  Prometheus + Grafana + BI tools instead.
- **UI as headless-operation requirement** — the CLI + trace
  replay remain the canonical headless surfaces.
- **Native dynamic-library plugins** (`dlopen`-style) — covered
  in the v1 non-goals; no change.
- **Unbounded marketplace remote-code-execution defaults** —
  signature + checksum + capability gates stay mandatory.
- **Provider-specific agent behaviour bypassing
  Tool/Trace/Policy contracts** — every new provider integrates
  through the existing surfaces.

## How to promote an item

When a v2 idea sharpens into something actionable:

1. Open a TODO entry under a `P11.x` (or beyond) segment in
   `TODOs.md` with explicit acceptance criteria.
2. Cross-reference back to the relevant theme letter in this
   file so the audit trail stays.
3. Delete the corresponding bullet here OR (preferred) leave a
   stub like `_promoted to P11.x — see TODOs.md_` so the
   history of the idea is preserved.

## Related

- [`RoadMap.md`](../RoadMap.md) — v1 direction + Later Tracks
  prose (`Later Tracks` items mirror the themes here).
- [`TODOs.md`](../TODOs.md) — active execution queue. The
  current P10 segment is the v1.0-rc.1 → v1.0 hardening arc;
  P11+ slots are reserved for promoted v2 items.
- [`docs/STABILITY.md`](STABILITY.md) — wire-shape promises
  this roadmap inherits.
- [`docs/WEB_UI.md` § Product positioning](WEB_UI.md#product-positioning) —
  the canonical debugger-only commitment that scopes Theme D.
- [`docs/archive/PROJECT_EVALUATION_2026-05-19.md` §7](archive/PROJECT_EVALUATION_2026-05-19.md) —
  the most recent eval that seeded the original v1.x bullets.
