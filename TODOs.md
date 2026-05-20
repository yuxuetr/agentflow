# AgentFlow TODOs

Last updated: 2026-05-19

## 维护约定

- 旧执行计划按时间分批归档到 `docs/archive/`：
  - `TODOs-archive-2026-05-09-n1-n10.md` — N1–N10 路线图段（已闭环）。
  - `TODOs-archive-2026-05-10-p0-p4.md` — 早期 P-段执行计划（已闭环）。
  - `TODOs-archive-2026-05-19-recently-closed.md` — 5/19 从 Recently Closed
    扫出去的中段历史。
  - `TODOs-archive-2026-05-20-closed-segments.md` — **本次 5/20 归档**：12
    个全 closed 的 P-段（P0/P1/P2/P3/P4/P5/P6/P7/P-H/P9/P-LLM/M）整体外迁。
- 本文件是短期执行队列，仅保留**活跃 P10 优化 backlog + 最近 closed 摘要**。
- 最新项目评估：`docs/archive/PROJECT_EVALUATION_2026-05-19.md`（A overall）。
- `docs/CURRENT_STATUS.md` 记录当前已实现状态。
- `RoadMap.md` 保留中长期路线。
- `HARNESS_MODE_EVOLUTION.md` 是 Harness Agent Mode 的设计规范；其
  可执行任务化展开（P-H 段）已闭环并归档。
- 任务状态只使用:
  - `TODO`: 未开始或正在执行。
  - `DONE`: 已完成、已测试、已提交。
  - `DEFERRED`: 显式推迟到 RoadMap Later Tracks 或 Non-Goals。

## Active Queue Overview

Current focus: **Core Runtime Stabilization + Harness Mode foundation**.

Near-term scope is CLI-first, Rust SDK-first, and local server/daemon-first.
Slack, Telegram, Discord, desktop tray, webhook channel routing, and other
channel integrations are intentionally deferred. Keep extension points open,
but do not implement channel adapters in this queue.

| Segment | Theme | Status |
| --- | --- | --- |
| P0 | V1 Contract Hardening | CLOSED |
| P1 | Security And Tool Governance | CLOSED (P1.1–P1.9 all DONE) |
| P2 | Local Server / Daemon Reliability | CLOSED (P2.1–P2.8 all DONE) |
| P3 | Rust SDK And CLI Experience | CLOSED (P3.1–P3.10 all DONE) |
| P4 | Memory, RAG, And Eval Foundations | CLOSED (P4.1–P4.7 all DONE) |
| P5 | Plugin, Marketplace, And Worker Hardening | CLOSED (P5.1–P5.8 all DONE) |
| P6 | Web UI Productization | CLOSED (P6.1–P6.5 all DONE) |
| P7 | Performance And Release Engineering | CLOSED (P7.1–P7.4-FU4 all DONE; v1.0.0-rc.1 tag unblocked) |
| P-H | Harness Agent Mode (parallel track) | CLOSED (H0–H5 all closed; H6 DEFERRED) |
| P9 | Dogfooding-Driven Refinements | CLOSED |
| P-LLM | Modality Provider Traits + Model Schema Cleanup | CLOSED (P-LLM.0–.5 all DONE; P-LLM.6 video DEFERRED) |
| M | Maintenance Tasks | CLOSED (M.1–M.7 all DONE; ongoing housekeeping) |
| **P10** | **Optimization Backlog (post-2026-05-19 evaluation)** | **NEW — active** |
| Deferred | Channel adapters / OS control / SaaS | non-goal |

## Recently Closed

- N9 (DashScope + DeepSeek + MiniMax wired into live nightly) —
  workflow run `26105740468` passed 24 / 24 in 21.88s with the
  newly-wired 3 providers + the original 6 all green:
  - DashScope · qwen-plus ✓ (Alibaba Bailian OpenAI-compat
    endpoint at `dashscope.aliyuncs.com/compatible-mode/v1`)
  - DeepSeek · deepseek-chat ✓ (V3 alias, `api.deepseek.com/v1`)
  - MiniMax · MiniMax-M2 ✓ (`api.minimaxi.com/v1` — note the `i`)
  All three drive through `OpenAIProvider::with_client(...)` with
  custom base URLs, mirroring GLM's test-layer-only pattern (no
  dedicated provider module). Per-provider helpers
  (`<provider>_live_lock` / `_base_url` / `_live_context`) and
  capability-profile rows added to `provider_consistency_live.rs`;
  workflow `env:` block exports `DASHSCOPE_API_KEY` /
  `DEEPSEEK_API_KEY` / `MINIMAX_API_KEY` from repository secrets;
  each test self-skips when its key is missing so forks without
  secrets stay green. The full 9-provider nightly verified by
  triggering `gh workflow run llm-live.yml` (no filter).
- N9 (live nightly CI verified end-to-end) — final dry-run
  `26103718043` after the model refresh and `max_tokens` headroom
  bump: **21 passed / 0 failed in 20.48s**. Every shipped provider's
  default text path now actually hits the real API on every nightly
  scheduled run + every `workflow_dispatch`:
  OpenAI `gpt-4o-mini` / Anthropic `claude-haiku-4-5` / Google
  `gemini-2.5-flash` / Moonshot `moonshot-v1-8k` / StepFun
  `step-1-8k` / GLM·Zhipu `glm-4.5-flash`. Capability gates
  (`AGENTFLOW_LIVE_MULTIMODAL_TESTS` / `_AUDIO_TESTS` /
  `_IMAGE_TESTS`) still keep vision / audio / image tests behind
  opt-in env vars to avoid cost surprises. N9 segment is fully
  closed.
- N9 (live nightly CI model refresh) — second dry-run after the
  init-validation fix passed 19 / 21 tests but surfaced 2 model-
  not-found 404s from vendor-side deprecations:
  Anthropic returned 404 for the hard-coded
  `claude-3-5-haiku-20241022` dated revision, Google returned 404
  for `gemini-1.5-flash` ("not found for API version v1beta"). Both
  fixed by updating the defaults in `provider_consistency_live.rs::
  run_text_path` to the rolling alias `claude-3-5-haiku-latest`
  (Anthropic) and the current stable `gemini-2.0-flash` (Google);
  both are also entries in the bundled `default_models.yml`. The
  `AGENTFLOW_LIVE_<PROVIDER>_TEXT_MODEL` env override path was
  already supported, so future drift can be patched at the workflow
  level without code changes.
- N9 (live nightly CI repro fix) — first dry-run of
  `LLM Live Smoke` revealed `AgentFlow::init()` in
  `prepare_live_provider` was force-validating EVERY provider in
  the bundled `default_models.yml` (including dashscope), so a
  single missing unrelated key would fail-close the entire suite
  even when only one provider was requested via the
  `workflow_dispatch` `providers` filter. Replaced the strict init
  call with a `~/.agentflow/.env` loader only — live tests already
  construct providers directly via `<Provider>::with_client(...)`
  and look up model names through the non-validating
  `LLMConfig::from_default_source()`, so strict init was redundant.
  Verified by `cargo check` + `cargo clippy -p agentflow-llm
  --tests -- -D warnings` locally; nightly workflow re-trigger is
  the next step after push.
- N9 (status reconciliation, no code change) — CLAUDE.md's N9
  status line was reconciled with shipped state. `.github/workflows/
  llm-live.yml` (live-LLM nightly CI) was already shipped in
  `68febae` and extended with GLM env aliases in `1afcd17`; with
  all 6 provider API-key secrets now configured (OPENAI / ANTHROPIC
  / GEMINI / MOONSHOT / STEPFUN / ZHIPU) the workflow runs end-to-
  end on schedule. N9 segment is closed; CLAUDE.md updated to drop
  the stale "Pending: live-LLM nightly CI" line.
- N9 (multimodal + tool_choice cross-provider invariants) —
  extends the 7 invariant tests landed in `1afcd17` with the two
  axes CLAUDE.md still listed as pending. New tests in
  `agentflow-llm/tests/provider_consistency.rs`:
  (1) `cross_provider_multimodal_paths_produce_uniform_response_shape`
  drives each of the 5 providers through its native multimodal
  request shape (OpenAI / Moonshot / StepFun via `image_url` parts;
  Anthropic via `image` parts with base64 `source`; Google via the
  OpenAI-style input its adapter rewrites to `inline_data`) and
  asserts the parsed `ProviderResponse` is uniform (text == "ok",
  `StopReason::Stop`, populated usage, empty `tool_calls`). Catches
  the drift where a multimodal adapter mis-parses the success
  response. (2–5) Four new
  `cross_provider_tool_choice_<variant>_is_honored_by_every_provider`
  tests (one per `ToolChoice` variant) drive all 5 providers and
  assert each captured request body contains the provider-specific
  mode-bearing field with the expected wire token (`auto` / `none` /
  `required`-or-`any` / the literal tool name). The `None` invariant
  is the highest-stakes — a provider silently dropping it would
  re-enable tool calls the caller explicitly forbade. Shared helper
  `drive_all_providers_through_tool_choice(choice)` mirrors the
  pattern from the existing `drive_all_providers_through_status`
  helper. `provider_tool_choice_field(provider, body)` abstracts
  Google's `toolConfig` field-path divergence from the canonical
  `tool_choice` key. Total: 5 new tests; `provider_consistency`
  suite now 56 / 56 (44 per-provider + 12 invariant). `cargo fmt
  --all` + `cargo clippy -p agentflow-llm --tests -- -D warnings`
  + `cargo test -p agentflow-llm --test provider_consistency` all
  clean.
- N8 (final follow-ups) — closed the two remaining items called out
  in CLAUDE.md's N8 status line. (1) Tool idempotency metadata for
  partial-resume auto-replay: new
  `AgentNodeResumeContract::from_result_with_tools(node, runtime,
  result, &ToolRegistry)` consults `Tool::idempotency()` /
  `ToolMetadata::with_idempotency` when params don't carry
  `_agentflow.side_effect_class`, so tools registered as `Idempotent`
  get `ReplayAllowed` on partial resume automatically; legacy
  `from_result(...)` retained as a thin wrapper for zero-impact on
  existing callers; `AgentNode::execute` (DAG path) and
  `build_skill_agent_outputs` (skill_agent path) both wired through;
  6 new bridge tests in
  `agentflow-agents/tests/agent_node_resume_contract.rs`. (2)
  `FlowValue::File` / `FlowValue::Url` checkpoint round-trip type
  fidelity: new `flow_value_file_and_url_survive_disk_round_trip`
  proves File/Url variants survive `CheckpointManager` save → load
  with full type fidelity (no silent collapse to `Json`); new
  `decode_checkpoint_flow_value` in `agentflow-core/src/flow.rs`
  distinguishes tagged-but-corrupt values (warn via `eprintln!`) from
  genuinely untagged legacy values (silent fallback), with 2 new
  tests pinning both behaviors. `cargo fmt --all`,
  `cargo clippy -p agentflow-core -p agentflow-agents -p agentflow-cli
  -- -D warnings`, and full test suites for those three crates all
  clean.

> Older Recently-Closed entries (P0 / P1 / P2 / P3 / P-H / P4 / P5 /
> P6 / P7 / M / P-LLM, all closed before this session) were moved to
> [`docs/archive/TODOs-archive-2026-05-19-recently-closed.md`](docs/archive/TODOs-archive-2026-05-19-recently-closed.md)
> on 2026-05-19 to keep this section focused on the latest changes.

---

## P10 — Optimization Backlog (post-2026-05-19 evaluation)

Action items derived from
[`docs/archive/PROJECT_EVALUATION_2026-05-19.md`](docs/archive/PROJECT_EVALUATION_2026-05-19.md)
§4 (per-crate gaps), §6 (remaining risks R15–R17), and §7.1–§7.3
(v1.0.0-rc.1 → v1.0 GA → v1.x recommendations).

All entries start as `TODO`. Promote individual items to a P10.x or
crate-named sub-segment only when picked up; the buckets below are
the long-form backlog, not the next sprint.

### P10.0 — v1.0.0-rc.1 release engineering (non-code ops, gates GA)

Each step below is a manual `ops` action. None are code work; they
all map to the P7.4-FU4 production-deployment checklist in
`docs/RELEASE_NOTES_v1.0.0-rc.1.md` DRAFT.

- TODO P10.0.1 Production deployment dress-rehearsal walkthrough
  - Run the 6-step `Production Deployment Checklist` from
    `docs/RELEASE_NOTES_v1.0.0-rc.1.md` on a fresh VM. Cross off
    each step + record `agentflow doctor --profile production
    --backup-check --format json` exit code.
- TODO P10.0.2 `cargo publish --dry-run` for all publishable crates
  - Order: `agentflow-core` → `agentflow-tools` → `agentflow-tracing`
    → `agentflow-llm` → `agentflow-nodes` → `agentflow-mcp` →
    `agentflow-memory` → `agentflow-rag` → `agentflow-agents` →
    `agentflow-skills` → `agentflow-harness` → `agentflow-viz` →
    `agentflow-db` → `agentflow-server` → `agentflow-worker` →
    `agentflow-cli`. xtask, ui not published.
- TODO P10.0.3 Tag `v1.0.0-rc.1`
  - One-way decision; human operator only.
- TODO P10.0.4 GitHub Release artifact + docker image push
  - Build per-arch CLI binaries, docker buildx image for
    `agentflow-server`, attach to GitHub Release.
- TODO P10.0.5 Fresh-VM doctor smoke
  - Provision a clean Ubuntu 24.04 VM with zero `~/.agentflow/`
    state, install released `agentflow` binary, run
    `agentflow doctor --profile production --backup-check`,
    expect exit code 0 (or document why not).

### P10.1 — agentflow-core (A — already strong, micro-polish only)

- TODO P10.1.1 (Stretch) Benchmark hot-path scheduler / FlowValue
  decode / checkpoint roundtrip
  - Criterion suites already exist (`benches/scheduler.rs`); compare
    against the perf-regression gate baseline and look for any 1.10×
    regressions accumulated during P3.3 envelope work.
- TODO P10.1.2 (Stretch) Document `decode_checkpoint_flow_value`'s
  warn-vs-silent fallback in `docs/CHECKPOINT_SCHEMA.md`
  - One paragraph; helps future readers understand pre-0.2 legacy
    handling and why the two paths diverge.

### P10.2 — agentflow-nodes (A-)

No active gaps from the evaluation. Future opportunities:

- TODO P10.2.1 (Stretch) Add a node-level latency bench for the
  16+ built-in nodes
  - Currently only `agentflow-core/benches/scheduler.rs` benches
    DAG mechanics; per-node hot-path benches would catch
    e.g. template-render regressions.

### P10.3 — agentflow-llm (A — but `init()` UX is the single biggest pre-GA fix)

- DONE P10.3.1 (HIGH — pre-GA) Lenient `LLMConfig::validate()` for
  missing provider keys (R15)
  - Landed: `LLMConfig::validate()` is now lenient — emits an
    `eprintln!` warning per missing-key provider naming the
    affected models, returns `Ok(())`. New
    `LLMConfig::validate_strict()` preserves the fail-close path
    for callers like `agentflow doctor --profile production`.
    `ModelRegistry::initialize_providers()` skips missing-key
    providers and tracks them in
    `missing_key_providers: HashSet<String>`;
    `ModelRegistry::get_provider()` consults that set and returns
    `LLMError::MissingApiKey` (actionable, names the env var) for
    skipped vendors instead of the misleading
    `LLMError::UnsupportedProvider`.
  - Tests: 5 new — 4 in
    `agentflow-llm/src/config/model_config.rs::tests`
    (`validate_emits_warning_but_no_err_for_missing_api_key`,
    `validate_strict_returns_missing_api_key_err_when_env_unset`,
    `validate_still_errors_on_unsupported_vendor`,
    `validate_still_errors_on_invalid_temperature`) + 1 in
    `agentflow-llm/src/registry/model_registry.rs::tests`
    (`load_config_skips_provider_with_missing_key_and_keeps_others`
    — proves the registry-level round-trip end-to-end). Full
    `cargo test -p agentflow-llm` green (216 tests / 0 failed);
    `cargo clippy -p agentflow-llm -p agentflow-cli -p
    agentflow-agents -p agentflow-harness --tests -- -D warnings`
    clean. CHANGELOG.md "Changed" section documents the migration
    note for callers depending on strict-init semantics.

- TODO P10.3.2 (Medium — v1.x) Promote DashScope / DeepSeek /
  MiniMax to dedicated provider modules (R16)
  - Current: 4 OpenAI-compat vendors (GLM + these 3) share
    `OpenAIProvider` via `create_provider`. Works for the wire
    shape match.
  - Trigger to do this: vendor introduces wire-format divergence
    that `OpenAIProvider` can't cleanly handle (custom tool-call
    format, multimodal extension, etc.).
  - Until then: keep the shared-adapter approach. Estimate when
    needed: ~300-500 LoC per vendor.

- TODO P10.3.3 (Medium — v1.x) Provider-specific tokenizers
  - Today the `PricingTable` cost tracking + `RuntimeLimits`
    token budgets use char-count or rough heuristics. Wire each
    provider to its real tokenizer (tiktoken for OpenAI, etc.) for
    accurate budget enforcement.

- TODO P10.3.4 (Low — v1.x) Auto-rotate live nightly default models
  on 404
  - The live nightly went through 4 rounds of vendor-side model
    deprecation in May 2026 (claude-3-5-haiku-20241022 →
    claude-haiku-4-5; gemini-1.5-flash → gemini-2.0-flash →
    gemini-2.5-flash; deepseek-chat → deepseek-v4-flash).
  - Build a `cargo xtask refresh-live-models` that pings each
    provider's models-list endpoint and rotates the workflow `env:`
    block's `AGENTFLOW_LIVE_<PROVIDER>_TEXT_MODEL` overrides.
  - Reduces manual `chore` toil after vendor deprecation
    announcements.

### P10.4 — agentflow-tools (A-)

No gaps from the evaluation. Future opportunities:

- TODO P10.4.1 (Stretch) Sandbox profile per-tool override
  - Today `security.os_sandbox` is a manifest-level flag. Future:
    `[security.tools.<name>] os_sandbox = "enforcing"` to override
    per individual tool when the skill needs heterogeneous
    enforcement.

### P10.5 — agentflow-mcp (A-)

- DONE P10.5.1 (Medium) Remove `client_old` historical baggage
  - Audit confirmed zero external callers in the workspace (both
    modules were `#[doc(hidden)]` since their introduction and never
    re-exported at crate root). Deleted `agentflow-mcp/src/client_old.rs`
    (182 lines) + `agentflow-mcp/src/transport.rs` (150 lines; only
    ever consumed by `client_old`).
  - Scope widened beyond the TODO's named module to also rename
    `transport_new` → `transport` so the post-cleanup name is
    internally consistent (the `_new` suffix existed precisely as
    contrast to the deleted old `transport`). All 10 affected
    callsites (6 tests + 2 examples + 2 internal modules) flipped
    via `sed` — `cargo build --workspace --tests` clean afterwards.
    `#[deprecated]` `pub use transport as transport_new;` re-export
    preserves the old import path for any 3rd-party caller through
    the transition window; they get a deprecation warning instead
    of a hard break. A 1-test `compat_tests` module pins the
    alias's type identity (boxing a `transport::MockTransport` into
    a `&dyn transport_new::Transport` only type-checks when both
    paths point at the same trait) so the re-export can't silently
    degrade.
  - lib.rs architecture doc updated to drop legacy mentions;
    `traits.rs` doctest is now consistent with the module name
    (was always `use agentflow_mcp::transport::Transport` — was
    forward-looking before, accurate now). Updated
    `OVERALL_EVALUATION_REPORT.md` to note the cleanup landed;
    `docs/MCP_TEST_EXAMPLES_GUIDE.md` paths updated.
    `agentflow-mcp` is below the stability tier line per
    `docs/STABILITY.md`, so this rename is in scope.
  - Tests: 217 mcp tests pass (incl. 1 new compat test, all
    doctests, integration tests using the new path); `cargo
    clippy -p agentflow-mcp --tests --examples -- -D warnings`
    clean; full workspace `cargo build --workspace --tests` green.
- DONE P10.5.2 (Medium — v1.x) Promote MCP server from
  `experimental` to `beta`
  - Closed method set: `initialize` / `notifications/initialized`
    / `tools/list` / `tools/call`. New methods may be added in
    minor releases; the existing four stay wire-stable. Required
    response fields: `initialize` → `result.protocolVersion` +
    `result.capabilities` + `result.serverInfo.{name,version}`;
    `tools/list` → `result.tools[]` with `{name, description,
    input_schema}` per item; `tools/call` success → `result.
    content`; `tools/call` failure → `error.{code,message}`
    envelope. Notifications return no response (`Option::None`).
    Error codes: `-32601` method-not-found, `-32603` tool-
    execution-failed.
  - New public surface: `MCPServer::handle_request` is now `pub`
    (single request → response entry point; the stdio loop is a
    thin wrapper around it, so non-stdio transports drive the
    same logic). `STABLE_PROTOCOL_VERSION: &str = "2024-11-05"`
    is the wire-reported protocol version — bumping it is the
    explicit signal that the Beta contract changed.
  - Tests: 6 fixture-driven compat tests
    (`tests/fixtures/server_contracts/*.json` × 6) + 2 invariant
    tests in `tests/server_contracts.rs`. The fixture format
    pins required fields (dotted paths) + exact values + error
    envelope shapes but tolerates additive fields, matching the
    Beta promise from `docs/STABILITY.md`. One `#[test]` per
    fixture for clean per-method failure diagnostics. The
    `initialize_protocol_version_matches_public_constant` test
    pins the constant ↔ wire-value equality so the two can't
    drift. The `fixtures_tolerate_additive_response_fields` test
    proves the harness honours the "additive fields OK" promise.
  - `docs/STABILITY.md` updated: new "MCP server" row in the
    Trace and Server APIs table + new fixture-ownership row.
    `lib.rs` doc-comment + `server.rs` module doc updated to
    declare Beta with the closed-method-set + non-stable
    (example handler, stdio framing) lists. 226 mcp tests
    green; clippy clean.

### P10.6 — agentflow-rag (A-)

- DONE P10.6.1 (HIGH — pre-GA) Pluggable retriever trait
  - Landed: the `Retriever` trait was already in
    `agentflow-rag/src/eval/runner.rs`; this slice added two new
    in-tree impls in `agentflow-rag/src/eval/retrievers.rs`:
    `DenseEval` (in-memory cosine similarity over pre-embedded
    corpus + queries, with dim validation, zero-vector handling,
    and stable tie-break) and `HybridEval` (Reciprocal Rank
    Fusion over any two `Box<dyn Retriever>`, default `k=60`,
    configurable inner-k multiplier, deterministic tie-break by
    id ascending). Re-exported via `eval::mod`.
  - CLI wiring: `--retriever {bm25,dense,hybrid}` +
    `--embedding-model <name>` (default
    `text-embedding-3-small`). Dense/hybrid embed corpus +
    queries once via `OpenAIEmbedding::embed_batch` before the
    sync eval runner consumes them — no async runtime inside
    `Retriever::search`. Title + body concatenation matches the
    BM25 path so backend comparisons stay apples-to-apples.
    Queries are deduped on text before embedding to cut cost.
    Hybrid composes `Bm25Eval` + `DenseEval` via `HybridEval`.
  - **No Qdrant required** — the TODO's 400-600 LoC estimate
    included a vector-store integration that turned out
    unnecessary at eval scale. Production-scale retrieval still
    uses `VectorStore` (Qdrant) directly; eval-scale (<100k
    docs) fits in RAM and benefits from determinism.
  - Tests: 10 new in `eval::retrievers::tests` (DenseEval:
    cosine ranking / unknown-query / dim mismatch / empty
    corpus / zero query vector; HybridEval: both-backends
    promotion / disjoint RRF / k cap / zero k / canonical
    "two moderate ranks beat one strong" / custom multiplier) +
    1 new in `commands::rag::eval::tests`
    (`build_dense_retriever_errors_without_openai_api_key`
    proves the missing-key error is single-line + actionable).
    All tests use mock vectors / mocked env — no real API call
    needed at test time. `cargo test -p agentflow-rag --lib`
    131/131 green; `cargo test -p agentflow-cli --features rag
    --lib commands::rag` 12/12 green; `cargo clippy -p
    agentflow-rag -p agentflow-cli --features rag --tests
    -- -D warnings` clean. `docs/RAG_EVAL.md` updated with the
    new backend section and dense/hybrid CLI examples.
  - Real-environment dependency: an end-to-end
    `agentflow rag eval --retriever dense` run against a real
    dataset needs `OPENAI_API_KEY` set; the CLI errors out
    early with a clear message naming the env var when it's
    missing. Hermetic / CI runs continue to use `--retriever
    bm25`, which has no external dependencies.

- TODO P10.6.2 (Medium) Additional eval baselines
  - Now unblocked by P10.6.1. Ship `dense.json` + `hybrid.json`
    baselines alongside the existing `bm25.json` so regressions
    across all three retrievers gate releases. Requires
    `OPENAI_API_KEY` to generate the baselines, but the on-disk
    snapshots themselves are deterministic-enough to check in
    once.

- TODO P10.6.3 (Low — Stretch) Latency profile per chunk size
  - Today the eval reports p50/p95 latency but not per-chunk-size.
    Add a benchmark dimension so chunking strategy regressions
    surface.

### P10.7 — agentflow-memory (B+)

- DONE P10.7.1 (Medium) `agentflow memory prune` CLI command
  - Landed: new top-level `Commands::Memory` subcommand with
    `prune --layer {preference,entity_facts} --db <path>
    --older-than <duration> [--format text|json-envelope]`.
    Backed by `agentflow-cli/src/commands/memory/prune.rs::execute`
    which dispatches to `SqlitePreferenceStore::open(&path) ->
    prune_older_than(cutoff)` or `SqliteEntityFactStore::open(&path)
    -> prune_invalidated(cutoff)` (preserves the trait's
    "active facts never touched" safety invariant). Defaults
    `--db` to `~/.agentflow/memory.db` matching the agent-runtime
    convention.
  - Duration parser: `<integer><unit>` where unit ∈
    `{s, m, h, d, w, y}`. Retention windows are
    days / weeks / years so the parser deliberately supports
    longer units than the workflow-level `parse_duration`
    (which tops out at minutes). Bare integers (`--older-than
    30`) are rejected because silently choosing a unit would
    turn a typo into data loss. Year uses 365.25 × 86 400 =
    31 557 600 s to track the Julian year without drift over
    multi-year spans.
  - Out of scope: session + semantic layers expose per-session
    clear instead of retention-based prune. They can join the
    surface once the trait gains a matching method (separate
    follow-up — touching `MemoryStore` stable API).
  - Tests: 6 unit tests in `commands::memory::prune::tests`
    (parser: every unit / bare-integer rejection / unknown-unit
    rejection / empty rejection / zero accepted; +1 in-crate
    round-trip via `SqlitePreferenceStore::in_memory` proving
    old-row pruned + fresh-row survives) + 5 hermetic CLI tests
    in `agentflow-cli/tests/memory_prune_tests.rs` that drive
    the CLI binary against real on-disk SQLite files seeded via
    the public memory-crate API: preference round-trip,
    entity_facts "active rows never touched" invariant,
    unsupported-layer rejection, missing-db rejection,
    bare-integer rejection. `cargo clippy -p agentflow-cli
    --tests -- -D warnings` clean.

- TODO P10.7.2 (Low — v1.x) Encryption-at-rest implementation
  - `EncryptedPreferenceStore` trait stub is in place per the P4.5
    design doc. Pick a KMS strategy (envelope encryption via
    `age` / `sops` / cloud KMS?) and ship a real impl.

- TODO P10.7.3 (Low — v1.x) Cross-session memory linking strategy
  - The 4-layer design separates Session / Semantic / Preference /
    Entity facts cleanly. A "memory graph" linking entities across
    sessions is a v2 design conversation.

### P10.8 — agentflow-agents (A-)

No active gaps. Future opportunities:

- TODO P10.8.1 (Stretch) ReAct trace replay diff tool
  - `agentflow trace replay` exists; an
    `agentflow agent replay --diff <baseline>` would compare a
    fresh ReAct run against a golden trace and surface step-level
    divergence (tool-call order, message tokens, stop-reason).

### P10.9 — agentflow-skills (A-)

- DONE P10.9.1 (Medium) Promote `skill inspect --with-mcp-discovery`
  to default-on
  - Landed: MCP discovery is now default-on whenever
    `--explain-permissions` is set and the manifest declares MCP
    servers. New `agentflow-cli/src/commands/skill/mcp_discovery_cache.rs`
    persists a manifest-level cache to
    `~/.agentflow/cache/skill_mcp_discovery.json` (24h TTL, single
    JSON document keyed by `hash_mcp_servers(...)`, schema-versioned
    so future bumps drop old entries silently). The hash inputs are
    `name`/`command`/`args`/`env` (the fields that affect what
    tools the server advertises); `timeout_secs` /
    `max_concurrent_calls` are deliberately excluded so adjusting
    them doesn't invalidate the cache. An `indicatif` spinner runs
    during fresh discovery so operators see something is happening
    during the spawn.
  - Flag surface: `--no-mcp-discovery` (opt-out),
    `--refresh-mcp-cache` (force re-spawn ignoring cache),
    `--with-mcp-discovery` (deprecated no-op + stderr warning that
    names the new flag). The summary line now identifies which
    path was taken: `cache hit` / `fresh discovery (cached for
    next run)` / `forced re-discovery (--refresh-mcp-cache)` /
    `skipped`, so operators see whether they paid the cost or
    not. Cache write errors are non-fatal — logged to stderr but
    don't fail the inspect call (next run just re-discovers).
  - Tests: 13 unit in
    `commands::skill::mcp_discovery_cache::tests` (hash stability
    across env iteration / server ordering; hash distinguishes
    argv / command / env-value changes; hash ignores timeout;
    load/save round-trip; load returns empty on missing file /
    schema mismatch / malformed JSON; TTL fresh/stale/unknown)
    + 4 hermetic CLI tests in
    `agentflow-cli/tests/skill_inspect_mcp_discovery_tests.rs`
    (deprecation warning fires; baseline emits no warning;
    `--no-mcp-discovery` short-circuits the spawn — proven by
    writing a SKILL.md whose server script doesn't exist so any
    real spawn would fail loudly; stray-flag-without-
    `--explain-permissions` note). `cargo clippy -p agentflow-cli
    --tests -- -D warnings` clean. `sha2` promoted from
    `[dev-dependencies]` to `[dependencies]` for the cache hash.

- TODO P10.9.2 (Low — Stretch) Skill marketplace search UX
  - Today `agentflow marketplace search` is text-only. Optional:
    JSON-envelope output + Web UI marketplace browser tab.

### P10.10 — agentflow-harness (A-)

- TODO P10.10.1 (Medium — v1.x) Promote individual H6 items from
  `Later Tracks` on concrete demand
  - Slash-command ecosystem expansion
  - TUI product shell (separate from CLI run)
  - OpenHarness-style config import
  - Plugin compatibility adapters
  - Provider subscription bridge
  - Each requires its own RFC. Don't pull en bloc.

- TODO P10.10.2 (Low — Stretch) Harness session replay
  - `harness list` shows session ids; a
    `harness replay <id> --speed 2x` would re-stream the JSONL
    log through a JSONL→TUI renderer for debugging long sessions
    after the fact.

### P10.11 — agentflow-cli (A-)

- DONE P10.11.1 (Medium — pre-GA) `agentflow workflow logs <run_id>`
  SSE follow command
  - Landed: new `WorkflowCommands::Logs { ... }` subcommand wired
    through `agentflow-cli/src/commands/workflow/server_ops.rs::logs`,
    backed by two new `ServerClient` methods:
    `list_events_history(run_id, after_seq)` (calls
    `GET /v1/runs/{id}/events/history`) and
    `stream_events_sse(run_id, after_seq, on_event)` (opens the
    `GET /v1/runs/{id}/events` SSE stream with a dedicated
    no-timeout reqwest client, parses `data:` lines per the SSE
    spec, and dispatches each event through a `FnMut` callback).
    Supports `--follow`, `--after-seq`, and `--format
    text|json|json-envelope` (envelope rejected with a clear
    error when combined with `--follow` because an envelope is
    bounded and a follow stream is not).
  - Tests: 4 unit tests (`format_event_text_*`,
    `logs_rejects_follow_with_json_envelope_format`) + 4 SSE
    parser tests (`parse_sse_event_payload_*`) + 5 hermetic
    integration tests in
    `agentflow-cli/tests/workflow_logs_tests.rs` that spin up a
    minimal axum mock server (no Postgres required) and exercise
    history-text, history-jsonl, history-envelope, follow-stream,
    follow-rejects-envelope round-trips end-to-end via the CLI
    binary. All green; `cargo clippy -p agentflow-cli --tests
    -- -D warnings` clean. Cargo.toml gains `reqwest` `stream`
    feature + `futures = "0.3"`.

- DONE P10.11.2 (Medium — pre-GA) `agentflow skill run --server`
  server-backed mode
  - Landed: `SkillCommands::Run` gains `--server` / `--auth-token`
    / `--tenant` flags. When `--server` (or `AGENTFLOW_SERVER_URL`)
    is set, the dispatch arm routes to the new
    `agentflow-cli/src/commands/skill/server_ops.rs::run_via_server`
    helper backed by a new
    `ServerClient::submit_skill_run(skill_name, input)` method that
    targets `POST /v1/skills/{name}:run`. Polls
    `GET /v1/runs/{id}` until terminal (`succeeded` / `failed` /
    `cancelled`) and pretty-prints the row, mirroring the
    `workflow run --server` pattern. The positional argument
    shifts semantics: filesystem path in local mode, skill NAME
    (resolved by server's `AGENTFLOW_SKILLS_INDEX` catalog) in
    server mode — documented in the clap help text + module docs.
  - Local-only flag rejection: `--memory`, `--model`, `--session`,
    `--trace`, and the local-only `--output json` are all rejected
    BEFORE any HTTP call with a single-line actionable error that
    names where the operator should look (e.g. "the server uses
    the model declared in the skill manifest loaded by the
    catalog at AGENTFLOW_SKILLS_INDEX"). `--trace` rejection
    points the operator at `agentflow workflow logs <run_id>` for
    the server-side trace equivalent. Local mode tolerates
    `--auth-token` / `--tenant` being set (warns to stderr but
    falls back to local execution) — that's the kindest UX for
    operators who set `AGENTFLOW_SERVER_URL` then unset it.
  - Tests: 5 new unit tests in
    `commands::skill::server_ops::tests` (one per local-only flag
    + happy path) + 5 hermetic integration tests in
    `agentflow-cli/tests/skill_run_server_tests.rs` that spin up
    a minimal axum mock server (no Postgres, no real skill
    registry) and drive the CLI binary end-to-end:
    submit-and-poll, envelope-mode wrap, `--model` rejection,
    `--output json` rejection, and 404 ("skill not installed")
    propagation. `cargo clippy -p agentflow-cli --tests -- -D
    warnings` clean.

- TODO P10.11.3 (Low — Stretch) Remaining `--format json-envelope`
  migrations
  - Audit which commands still lack `--format json-envelope`
    (likely a few small ones like `mcp config`, `marketplace
    search`, `config show`). Migrate them so the envelope contract
    is universal.

- DONE P10.11.4 (Medium — pre-GA) Server-side mapping for
  local-only `workflow run` flags
  - Picked option (b): reject up front. Wiring `--model` /
    `--execution-mode` / `--max-concurrency` / `--input` /
    `--timeout` / `--max-retries` through to the server needs a
    schema change to `POST /v1/runs` body + executor honouring it
    end-to-end; tracked as a v1.x follow-up. For pre-GA the
    important thing is no silent drops.
  - Scope widened beyond the TODO's named 3 flags to cover the
    full silent-drop class: `--model`, `--execution-mode`
    (non-default), `--max-concurrency` (non-default), `--run-dir`,
    `--watch`, `--output`, `--input`, `--dry-run`, `--timeout`
    (non-default), `--max-retries` (non-zero). Two categories
    surface in the error messages: **always-local** (filesystem +
    in-process flow — each points at the concrete server-side
    alternative, e.g. `--watch` → `agentflow workflow logs <run_id>
    --follow`, `--dry-run` → `agentflow workflow validate <file>`)
    and **future API addition** (each names P10.11.4 so curious
    operators can find the follow-up).
  - Landed: new public
    `workflow::server_ops::reject_local_only_flags(...)` validator
    wired into the `WorkflowCommands::Run` dispatch arm in
    `main.rs` before the workflow file is read. Defaults must
    match the clap definitions; the validator only fires on
    explicit overrides.
  - Tests: 13 new unit tests in
    `commands::workflow::server_ops::tests` (one per flag + the
    baseline-passes path + guard ordering invariant proving
    always-local fires before future-API when both are set) + 11
    hermetic CLI tests in
    `agentflow-cli/tests/workflow_run_server_validation_tests.rs`
    (one per flag against an obviously-unreachable URL —
    validation runs before any network call, so the per-flag
    message proves the guard fired). `cargo clippy -p
    agentflow-cli --tests -- -D warnings` clean.

### P10.12 — agentflow-tracing (A)

No active gaps. Future opportunities:

- TODO P10.12.1 (Stretch) Hybrid TUI view (timeline + DAG side-by-
  side)
  - Today `trace replay` and `trace tui` are separate paths. A
    split-pane view that shows DAG topology on the left + step
    timeline on the right would be valuable for debugging fan-out
    workflows. (Web UI already has trace-compare for this.)

### P10.13 — agentflow-viz (B — needs a strategic decision)

- TODO P10.13.1 (Medium — v1.x) Decide: merge with `agentflow-ui`
  OR establish live-trace interop protocol
  - `agentflow-viz` is static-only (YAML → Mermaid / DOT / JSON).
    `agentflow-ui` already renders DAG with live state. Two
    options:
    - **Merge**: deprecate `agentflow-viz`, fold its rendering
      logic into the UI's static-export path. Smaller workspace.
    - **Live interop**: keep `agentflow-viz` as the "static export"
      crate and have the UI call into it for printable snapshots.
      Cleaner separation.
  - Either way, document the decision in `docs/WEB_UI.md`.

### P10.14 — agentflow-server (A-)

No active gaps beyond the v1.0.0-rc.1 ops (P10.0). Future:

- TODO P10.14.1 (Medium — v1.x) Per-run retention override via
  POST body
  - Today retention is per-tenant + per-profile. P2.2 left
    per-run override as deferred. A `retention_overrides:
    {events_days, artifacts_days}` field on `POST /v1/runs` body
    would let users keep critical runs longer than the global
    sweep.

- TODO P10.14.2 (Low — v1.x) Operational dashboards (Grafana
  templates)
  - Server emits Prometheus metrics; a checked-in Grafana
    dashboard JSON would let operators import in 1 click. Today
    they have to build it themselves.

### P10.15 — agentflow-db (B+)

- TODO P10.15.1 (Medium — v1.x) Real backup/restore implementation
  - Today: docs (`SERVER_BACKUP_RESTORE.md`) + `agentflow doctor
    --backup-check` probes. Production backup is `pg_dump` +
    filesystem snapshot. An `agentflow backup --output <path>`
    CLI that orchestrates both would close the loop for operators.

- TODO P10.15.2 (Low — v1.x) Read-replica support
  - All repos write through the primary. For read-heavy gateways,
    a `--database-read-url` option that routes `list_*` /
    `get_*` reads to a replica would scale better.

### P10.16 — agentflow-worker (B)

- TODO P10.16.1 (Medium — v1.x) Signed-JWT identity flavour for
  worker admission (P5.5 deferred)
  - Today: PSK-only auth via `WorkerCredential`. JWT is documented
    as the next iteration when the broader auth track ships
    issuer/audience/key rotation primitives.

- TODO P10.16.2 (Low — v1.x) Worker pool admission heuristics
  - Today: `max_workers` + `max_concurrent_tasks_per_worker` are
    static. Add: capacity-aware load balancing, locality hints
    (`run_dir` co-location), and per-worker capability advertising
    (which node types each worker can run).

### P10.17 — agentflow-ui (B → "operator dashboard")

- TODO P10.17.1 (HIGH — v1.x) Decide product positioning
  - Today: "debugger" per the RoadMap. Real-world Web UIs for
    workflow platforms tend to grow into "operator dashboards"
    (cost / retry rates / policy decisions / worker utilization).
    Decide whether to commit to that productization arc or stay
    debugger-only.
  - Either way: write the answer in `docs/WEB_UI.md` so future
    contributors know the bar.

- TODO P10.17.2 (Medium — v1.x) Preference UI wiring to P6.4 API
  - The `/v1/preferences` API exists (P6.4). The UI still reads /
    writes localStorage. Switch to the API so preferences sync
    across browsers.

- TODO P10.17.3 (Medium — v1.x) Server-side `?filter=` pre-filter
  for very long runs
  - P6.5 client-side event filter works for <10k events. For
    longer runs the server should pre-filter via `/v1/runs/{id}/
    events/history?filter=...`.

- TODO P10.17.4 (Low — v1.x) Playwright suite in CI
  - The e2e specs exist (`agentflow-ui/e2e/`) but are not in
    `quality.yml` because they need Chromium + a live server +
    Postgres. Either: (a) ship a `docker-compose` test harness +
    new CI job, or (b) keep them as local-only smoke.

### P10.18 — xtask (A-)

No active gaps. Future opportunities:

- TODO P10.18.1 (Stretch) `cargo xtask refresh-live-models` (also
  listed under P10.3.4)
- TODO P10.18.2 (Stretch) `cargo xtask check-changelog` to ensure
  every PR touches `CHANGELOG.md` or has a `chore(skip-changelog)`
  marker

### P10.19 — Cross-crate / workspace level

- TODO P10.19.1 (HIGH — pre-GA) WASM plugin runtime evaluation
  - From eval §7.2 item #9. Subprocess JSON-RPC plugin runtime is
    stable. WASM is the natural "v2 plugin runtime" — sandbox is
    free, distribution is single-file, startup is faster.
  - Action: write a 1-pager comparing wasmtime / wasmer / extism
    plugin frameworks against the current subprocess `Plugin`
    trait surface. Decide whether to invest before v1.0 GA or
    push to v2.0.

- TODO P10.19.2 (Medium — v1.x) Workspace-wide perf regression
  detection
  - `bench-gate` exists for criterion benches. Extend to capture
    `cargo test --workspace` total wall-clock per crate and gate
    on 1.5× regressions to catch test-suite bloat early.

- TODO P10.19.3 (Low — Stretch) Centralized `docs/ROADMAP_v2.md`
  for post-v1.0 direction
  - Today the v1.x ideas are scattered across this file, the eval
    report §7.3, and `RoadMap.md` "Later Tracks". A consolidated
    v2 roadmap (once v1.0 GA cuts) would help align contributors.

---

> The 12 fully-closed P-segments (P0 / P1 / P2 / P3 / P4 / P5 / P6 /
> P7 / P-H / P9 / P-LLM / M) were archived to
> [`docs/archive/TODOs-archive-2026-05-20-closed-segments.md`](docs/archive/TODOs-archive-2026-05-20-closed-segments.md)
> on 2026-05-20 to keep this file focused on the active P10
> optimization backlog. Every closed entry is preserved verbatim;
> `git log -- TODOs.md` also surfaces the per-commit history.

---

## Deferred / Explicit Non-Goals

These are intentionally out of the current queue. Leave extension seams
where reasonable, but do not implement product features for them yet.

- DEFERRED Channel adapters:
  - Slack, Telegram, Discord, email, webhook routers, desktop tray, and
    multi-channel message normalization are deferred.
- DEFERRED Local OS control tools:
  - Screenshot, keyboard, mouse, clipboard, and window-management tools
    are deferred until security profiles, sandboxing, audit, and
    confirmation hooks are stronger.
- DEFERRED Full SaaS productization:
  - Organization management, billing, hosted multi-user UI, OAuth/JWT,
    background Skill updates, and channel-based routing are deferred.
- DEFERRED Native dynamic library plugins:
  - Subprocess JSON-RPC remains the only v1 plugin runtime. WASM is a
    later evaluation only after subprocess is exercised by real
    third-party plugins.
- DEFERRED P-H.H6 Harness advanced compatibility:
  - Promoted to RoadMap Later Tracks; individual items return here only
    when concretely required.

---

## Execution Notes

- Pick one item at a time, expand it into concrete subtasks if not
  already enumerated, then commit code and sync this file after each
  completed feature.
- For each new task added to this file, add at least one round-trip or
  smoke test that proves the acceptance criteria.
- Prefer the smallest coherent feature per commit; resist mixing P-H
  work into a P1/P2 commit.
- When in doubt about a placement, prefer keeping it in `RoadMap.md`
  until a concrete subtask emerges.

## Quality Gates

For each task:

- Read relevant code/docs first.
- Implement the smallest coherent feature.
- Run focused tests or validation commands.
- Commit the feature with a conventional message.
- Update this TODO file only after the feature commit succeeds.
- Cross-reference the task ID in the commit body (e.g., `Refs P-H.0`).

## Cross-References

- `RoadMap.md` — forward direction; Harness Agent Mode under Later
  Tracks mirrors this file's P-H section.
- `docs/CURRENT_STATUS.md` — what currently exists.
- `docs/STABILITY.md` and `docs/API_COMPATIBILITY.md` — stable surfaces.
- `HARNESS_MODE_EVOLUTION.md` — full Harness Mode design spec.
- `docs/archive/PROJECT_EVALUATION_2026-05-19.md` — **most recent** project
  evaluation (A overall, v1.0.0-rc.1 candidate; all N1–N10 closed).
- `docs/archive/PROJECT_EVALUATION_2026-05-14.md` — prior evaluation that
  drove the P6/P7/P-H/M segment additions (B+ overall).
- `docs/archive/PROJECT_EVALUATION_2026-05-01.md` — historical evaluation
  that drove the original P0-P5 task queue (B+ overall).
- `docs/archive/TODOs-archive-2026-05-20-closed-segments.md` — **most
  recent** archive: 12 fully-closed P-segments (P0–P9 + P-H + P-LLM + M)
  moved out of the active file on 2026-05-20.
- `docs/archive/TODOs-archive-2026-05-19-recently-closed.md` — Recently-
  Closed entries swept on 2026-05-19.
- `docs/archive/TODOs-archive-2026-05-09-n1-n10.md` and
  `docs/archive/TODOs-archive-2026-05-10-p0-p4.md` — N-series + early
  P-series execution-plan history.
