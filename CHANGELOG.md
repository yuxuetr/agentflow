# Changelog

All notable changes to AgentFlow are documented in this file. Format
loosely follows [Keep a Changelog](https://keepachangelog.com/), and
the project tracks [Semantic Versioning](https://semver.org/) at the
workspace level (most crates pin to 0.2.x or 0.3.0-alpha as of this
file's first entry).

## [Unreleased] — v0.3.0 candidate

The R1 → R4 reflection arc (see `docs/L1_L3_REFLECTION_R*.md`) drove
this set of changes. Most landed via a multi-session dogfooding loop:
build an application that exercises a platform surface, capture
findings as it goes, close the findings, repeat. **31 dogfooding
findings closed across 4 reflection cycles** (every
agentflow-internal item in the action queue), with no regressions
across the 200+ tests touched.

### Added

#### Workflow grammar

- **`type: shell` YAML workflow node** (F-A7-2 fully closed,
  `3c3ab02`). Wraps `agentflow_tools::ShellTool` with a
  `SandboxPolicy` built from YAML params. `allowed_commands` is a
  required schema field — workflows without an allowlist fail at
  parse time, no permissive-by-default arbitrary code execution.
  See `agentflow-cli/src/executor/shell.rs`.
- **`input_mapping` accepts `{{ item.* }}` lookups inside a map
  sub-flow** (F-A6-5, `54a2751`). Flat (`item.field`) and dotted
  (`item.foo.bar`) paths both supported. Encoded via the sentinel
  source-node id `"!item"`. Existing `{{ nodes.X.outputs.Y }}`
  lookups work unchanged.
- **Map node `max_concurrent: N` parameter** bounds simultaneously-
  running sub-flows via `tokio::sync::Semaphore` (F-A6-1,
  `a4e89e8`). Unbounded preserved as legacy default. `Some(0)` is
  rejected as a config error.
- **Map node `results_summary` sibling output** surfaces
  `{total, ok, err, err_indexes}` alongside `results` (F-A6-3,
  `fee8586`). Workflows can route on partial failure without
  walking nested JSON; `eprintln!` warning fires on any failure.
- **Template node auto-detects JSON output** when the rendered
  string starts with `[` or `{` (F-A6-7, `8b73298`). Parse failure
  falls back to String wrap (safe for prose). Explicit
  `output_format: "json"` preserved as strict mode.

#### CLI

- **Pluggable RAG eval retrievers** (P10.6.1):
  `agentflow-rag::eval::DenseEval` (in-memory cosine similarity over
  pre-embedded corpus + queries) and
  `agentflow-rag::eval::HybridEval` (Reciprocal Rank Fusion with
  configurable `k` and inner-k multiplier) join the existing
  `Bm25Eval` behind the `Retriever` trait. The CLI gains
  `--retriever {bm25,dense,hybrid}` plus `--embedding-model
  <name>` (defaults to `text-embedding-3-small`); dense and
  hybrid require `OPENAI_API_KEY` at run time and surface a
  single-line actionable error when it's missing. Eval-scale
  corpora (<100k docs) keep the full vector matrix in RAM — no
  Qdrant required for the eval harness. RRF tie-break is
  deterministic (score desc, then id asc) so paired sign-test
  comparisons across runs remain reproducible. 10 new unit
  tests in `eval::retrievers::tests` plus 1 hermetic CLI test
  (`build_dense_retriever_errors_without_openai_api_key`) cover
  the new code paths.
- **`agentflow skill run --server <url>`** dispatches the skill
  to a remote `agentflow serve` instance via
  `POST /v1/skills/{name}:run` (P10.11.2). Mirrors the
  `workflow run --server` pattern: submits, polls
  `GET /v1/runs/{id}` until terminal status (succeeded / failed /
  cancelled), and pretty-prints the final row. The positional
  argument shifts semantics in server mode — it's the skill NAME
  resolved via the server's `AGENTFLOW_SKILLS_INDEX` catalog, not
  a local filesystem path. New flags: `--server <url>`,
  `--auth-token <token>`, `--tenant <id>`. Local-only flags
  (`--memory`, `--model`, `--session`, `--trace`) are rejected
  with a single-line actionable error when combined with
  `--server` because the wire contract doesn't accept per-request
  overrides today. `--output` accepts `json-envelope` in server
  mode (canonical `CliJsonEnvelope`); the local-mode `json`
  value is rejected to keep the wire-schema surface narrow.
  Hermetic axum mock-server integration tests cover the
  submission round-trip, envelope wrap, local-only flag
  rejection, and 404 propagation.
- **`agentflow workflow logs <run_id>`** subcommand consumes the
  server's persisted event log (P10.11.1). Without `--follow`,
  fetches the history snapshot as a single JSON array via
  `GET /v1/runs/{id}/events/history`. With `--follow` (`-f`),
  opens an SSE stream against `GET /v1/runs/{id}/events` and
  prints each event as it arrives until the server closes the
  connection. Supports `--after-seq <n>` for resuming reconnects,
  `--format text|json|json-envelope` (envelope incompatible with
  `--follow` — rejected with a clear error since envelopes are
  bounded and follow streams are not), `--server` / `--auth-token`
  / `--tenant` matching the other server-backed `workflow`
  subcommands. Hermetic round-trip tests via a tiny axum mock
  server (no Postgres required).
- **`agentflow harness run --approve {none,cli,auto-allow,auto-deny}`**
  wires `HookedTool` into the CLI Harness path (F-A2-11, `9d386b3`).
  Combined with `--profile production`, every NonIdempotent tool
  call surfaces an interactive operator prompt. Default `none`
  preserves legacy behaviour for back-compat.
- **`agentflow skill run --output {text,json}`** emits a single
  JSON object on stdout suitable for piping into jq / downstream
  tooling (F-A2-6, `9a96058`). Banners go to stderr in JSON mode;
  `--trace` inlines the runtime trace under the `trace` key.
- **`agentflow doctor` opens its Config section with a
  human-readable source label** ("`/path/to/models.yml` (overrides
  built-in)") instead of the bare Rust-debug enum (F-A7-4,
  `bdaff36`). JSON output gains `models_config_source_kind` as a
  stable snake_case enum.
- **`agentflow llm models --refresh-from-api`** live-queries each
  OpenAI-compatible provider's `/v1/models` endpoint and prints the
  delta vs the local registry (F-A7-6, `6290ca9`). Output groups
  per-provider: `new` (provider-side additions to add to
  `models.yml`), `only_local` (deprecated / typo / private
  deployment), `shared` (count). Read-only; respects `--provider`
  filter. Currently supports openai / moonshot / stepfun /
  dashscope. 5 new unit tests on URL construction + truncation.

#### Agent runtime

- **ReAct steering note on repeat tool calls** (F-A2-13, `d7651f7`).
  When the LLM returns the exact same `(tool, params)` two
  iterations in a row, the second tool result memory message gets
  an `[agentflow steering note (F-A2-13): ...]` nudging the model
  to advance. Advisory only — tool still runs. Trace-side
  `AgentStepKind::ToolResult` stays clean.

#### Skill manifest

- **SKILL.md frontmatter `model:` field now honoured** (F-AF-2,
  `100c267`). Was silently dropped because the field wasn't on
  `SkillMdFrontmatter`. Empty/whitespace strings collapse to
  `None` so a stray `model: ""` doesn't propagate.

#### Examples

- **`examples/applications/code-reviewer-write/`** (`83a9765`)
  end-to-end Harness Mode approval gate validation binary; uses
  `wrap_registry` + `CliApprovalProvider` to exercise the
  approval flow against real shell + file:write tools. Includes
  `--auto-approve` for CI smoke and `--prefetch-diff` workaround
  for moonshot-v1-128k's loop pathology.
- **`examples/applications/research-assistant/`** (`b492f6a`)
  L1 binary fetching arxiv papers, deduping via
  `SqliteEntityFactStore`, summarising via a single LLM call.
- **`examples/applications/doc-translator/`** — full A6 spec
  shipped in 4 iterations:
  - iter 1 (`141b993`): `map parallel + LLM` primitive validator,
    4 langs hardcoded.
  - iter 2 (`4b882eb`): real file I/O, 2 files × 4 langs.
  - iter 3 (`ec2c15d`): `file_list × lang_list` cross-product via
    Tera template.
  - iter 4 (`35c1d01`): real file discovery via the new
    `type: shell` node. Adding a markdown file to `input/` is a
    zero-line workflow change.

### Changed

- **Default template node behaviour**: auto-detects JSON when
  rendered output starts with `[`/`{`. Workflows that already set
  `output_format: "json"` are unaffected; new workflows can omit
  the hint. Parse failure falls back to legacy String wrap (safe).
- **Validator behaviour for template nodes**: arbitrary
  `parameters` keys no longer false-warn (F-A6-6, `8b73298`).
  Template's whole point is to consume arbitrary Tera context.
  Typo-detection for other node types (closed ParamSpec) is
  unaffected.
- **`LLMConfig::validate()` is lenient on missing API keys**
  (P10.3.1). Previously, `AgentFlow::init()` against the bundled
  `default_models.yml` would fail-close for a fresh user with
  only `OPENAI_API_KEY` set, because the YAML references ~9
  providers and the strict validator required every key to be
  present. Now `validate()` emits an `eprintln!` warning naming
  each missing provider + the affected models, and returns
  `Ok(())` so init proceeds. The fail-fast moves to the lookup
  path: `ModelRegistry::get_provider("anthropic")` now returns
  `LLMError::MissingApiKey { provider: "anthropic" }` (actionable
  — names the env var to set) when that provider was skipped at
  init time, rather than the misleading `UnsupportedProvider`.
  **Migration**: callers that need the old fail-close semantics
  (e.g., `agentflow doctor --profile production` health checks)
  should use the new `LLMConfig::validate_strict()` method, which
  returns `Err(LLMError::MissingApiKey)` on the first missing
  configured key. Structural validation (unsupported vendor,
  out-of-range numeric fields) remains a hard error in both
  paths.

### Fixed

- **`HarnessProfile::Local` silent-allow footgun documented**
  (F-A2-12, `c552d3c`). Rustdoc on the enum variants + a footgun
  callout in `docs/HARNESS_MODE.md` make it clear that without
  `.with_profile(HarnessProfile::Production)`, the approval gate
  doesn't fire for NonIdempotent tools.
- **`agentflow workflow validate` no longer false-warns on map's
  `input_list` / `max_concurrent`** (F-A6-2, `a4e89e8`). ParamSpec
  list bumped.
- **`agentflow skill run` answer recovery from truncated JSON**
  (F-A2-1, `ec3e1a7`). Best-effort `answer` field extraction in
  `react/parser.rs` when `max_tokens` truncates the response
  mid-JSON. 6 new tests.
- **Doctor integration test patched** for the F-A7-4 output-format
  change (`8b73298`). Previously only the unit tests covered it.
- **94 text models bumped from `max_tokens: 4096` to `32768`**
  (F-A7-8, `42c3225`). 1M+ context models can sustain much
  longer outputs; the previous cap was too conservative.
- **`LLMError::MissingApiKey` renders an actionable one-liner**
  (F-AF-4, `6b89317`). Names the provider-specific env var (e.g.
  `MOONSHOT_API_KEY (or MOONSHOT_KEY)`), points at
  `~/.agentflow/.env`, suggests `agentflow config init` to
  generate the template, and references the docs. New
  `env_var_hint(provider)` helper has table coverage for 6
  providers with a generic fallback. 3 new unit tests.
- **A1.5 persona now re-measures LUFS before save** (F-EX-1,
  `b35f371`). Adds a step 6 audio_loudness call after
  normalize_lufs / fade so the final report uses the **实测**
  value rather than the target parameter (integrity issue
  caught in R1 dogfooding).

### Removed

- **6 dead `agentflow-llm/config/models/*.yml` files** (F-A7-3,
  `5578743`). The vendor_configs split was never read by the
  runtime registry. 4 misdirecting docs updated to point at the
  real source (`templates/default_models.yml`).

### Docs / conventions

- **R1 → R4 reflection sequence** (`docs/L1_L3_REFLECTION_*.md`,
  `b3ee990` / `2d3d06d` / `edd9572`). R2 froze the L1↔L3
  selection rule + per-application matrix; R3 documented the
  R2-follow-up sweep + 6 emergent patterns; R4 captured the A6
  sweep + 6 more patterns. R4 §5 has the cumulative arc.
- **Examples conventions** (`examples/README.md`) gained two
  cross-cutting bullets: LLM-judgement output is
  non-deterministic — run multiple times, union the findings
  (F-A2-5, `0d921aa`); translation workflows must guard
  `source_lang != target_lang` before LLM dispatch (F-A6-4,
  `907b6e7`).
- **`docs/HARNESS_MODE.md`** got a footgun callout for the
  Local-vs-Production approval-gate asymmetry + an inline comment
  in the canonical snippet (F-A2-12, `c552d3c`).
- **`docs/AGENT_SDK.md` gained two reference sections**
  (`b35f371`): a `FlowValue` field reference table enumerating
  exact field names per variant (F-DOC-2; prevents `media_type`
  vs `mime_type` round-trips), and a "Loading
  `~/.agentflow/.env` from standalone binaries" canonical 6-line
  snippet (F-A7-7; standardises the pattern used by every
  standalone example binary).
- **`agentflow-llm/README.md` § Moonshot** documents the
  kimi-k2.6 `temperature: 1.0` constraint with the exact API 400
  error message (F-A7-5, `b35f371`), plus the org-level
  concurrency limit of 3 that motivates `max_concurrent: 3` in
  `map parallel` workflows.

### Known still-open

- **F-A6-8** (Tera library quirk, not an agentflow bug): Tera
  `loop.parent.*` introspection doesn't work in this Tera
  version. `set_global` accumulator is the documented workaround
  in `examples/applications/doc-translator/workflow-iter3.yml`.
- **A6 iter 5** (100+ file stress test): all platform capability
  axes are now in place; iter 5 is a quantity question rather
  than a capability one. Probably worth running once before any
  v0.3.0 cut.
- **Phonon-external items** (F-PH-1/2/3, plus F-DOC-3/4 docs that
  live phonon-side): not in the agentflow workspace; tracked
  separately.
- **All other agentflow-internal items from the R1 → R4 sweep
  are now closed** (F-DOC-2, F-A7-5, F-A7-6, F-A7-7, F-AF-4,
  F-EX-1 all landed; see Added / Fixed / Docs above).

---

## [0.2.0] and earlier

No structured changelog kept before this entry. For change history
prior to v0.3.0 prep, see `git log` (most commits follow
Conventional Commits) and `docs/archive/PROJECT_EVALUATION_2026-05-19.md`
for the most recent cumulative project evaluation (or
`docs/archive/PROJECT_EVALUATION_2026-05-14.md` for the prior one).
