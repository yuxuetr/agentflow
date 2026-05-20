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

#### CLI ops

- **`agentflow backup --output <path>`** (P10.15.1). Orchestrates
  `pg_dump --format=custom` + `tar -czf` of the five filesystem
  state surfaces (`run_dir`, `trace_dir`, marketplace cache,
  skills, plugins) into a single bundle directory with a
  versioned `manifest.json`. Closes the operator loop that
  `docs/SERVER_BACKUP_RESTORE.md` documents — that doc described
  *which* state surfaces must be backed up; this command actually
  does it in one invocation instead of leaving the operator to
  run six commands by hand and reason about the order.
  - Flags: `--output` (required), `--database-url`, `--include`
    (repeatable; aliases like `runs` → `run_dir`, `database` →
    `db` accepted), `--dry-run`, `--force`,
    `--format text|json|json-envelope` (canonical `agentflow.cli/1`).
  - Manifest schema discriminator `agentflow.backup/1` is the
    wire-shape promise a future `agentflow restore --input <path>`
    will consume. Restore itself stays out of this TODO's scope.
  - Tool requirements (`pg_dump`, `tar`) are PATH-probed up front;
    "tool not found" surfaces as a `failed` step with a
    package-manager hint, not an unhelpful panic.
  - A missing source directory is `skipped` (not `failed`) — the
    common case where the operator only opted into a subset of
    state surfaces stays out of the failure path.
  - 12 hermetic unit tests in `commands::backup::tests`:
    include-name parsing + aliases, artifact-name layout
    discipline, URL password redaction (3 variants), dir-prep
    refuse/force/create/dry-run paths, end-to-end dry-run
    behavior (all 6 includes), explicit-include subset, and
    DB-step skip-without-`DATABASE_URL`. Postgres / tar are
    never invoked in the test suite, so this runs hermetically
    in CI.

#### Worker admission

- **Signed-JWT identity flavour for worker admission** (P10.16.1).
  New `agentflow-server::scheduler::jwt` module ships `JwtPolicy`
  (issuer / audience / key pool / leeway), HS256 + RS256
  `JwtVerificationKey`, and a strict claim validator
  (`iss`/`aud`/`sub`/`exp`/`nbf`). `WorkerAdmissionPolicy` gains
  `jwt: Option<JwtPolicy>` + `jwt_workers: HashSet<WorkerId>` so
  workers can opt into JWT auth alongside (or instead of) the
  existing PSK path. PSK takes precedence over JWT when a worker
  is misconfigured into both sets so there's no silent
  downgrade. The `aud` claim deserializer is tolerant of both
  the string and string-array forms per RFC §4.1.3. Key
  rotation works the same way as PSK: append a new
  `JwtVerificationKey` to the policy pool, flip the IdP, drop
  the old key. HS256 fits self-administered deployments; RS256
  fits the production path where an external IdP (Okta / Auth0
  / Vault / GCP Workload Identity) signs and the control plane
  only holds the public key.
  - **Wire-shape additive change:** `AdmissionError::InvalidCredential`
    gained a `reason: String` field so the verifier-specific
    failure mode (PSK rotation mismatch / JWT issuer mismatch /
    expired / etc.) reaches the operator-facing log line. The
    contract is `Experimental` per `docs/STABILITY.md` so this
    is in scope; no external matches exist in the workspace.
  - 14 hermetic unit tests in `scheduler::jwt::tests` cover
    happy path, every documented failure mode, leeway boundary,
    key rotation pool, and the multi-`aud` shape; 7 more tests
    in `scheduler::admission::tests::jwt_flavor` exercise the
    policy-layer routing (valid token admitted, missing
    credential, wrong subject, expired token,
    `jwt_workers`-without-`jwt`-policy as server config error,
    PSK-takes-precedence, anonymous workers still anonymous
    when JWT is configured). `now()` injection keeps the suite
    deterministic.
  - gRPC-metadata propagation of admission tokens is still
    deferred (separate TODO).
  - Adds `jsonwebtoken = "9.3"` dep to `agentflow-server`.

#### Server gateway

- **Per-run retention override on `POST /v1/runs`** (P10.14.1).
  The request body now accepts an optional `retention_overrides`
  object with `events_days` and `artifacts_days` fields. The
  cleanup sweep uses `max(global, override)` so a per-run
  override can only *extend* retention — it cannot shorten the
  tenant + profile default. Pinning a run's events or artifacts
  also pins the parent `runs` row itself: the cleanup SQL keys
  the run-row deletion on `GREATEST(global, events_override,
  artifacts_override)` so the `ON DELETE CASCADE` from `runs`
  doesn't yank the pinned children out from under the override.
  Negative overrides are rejected at the API layer with a clean
  `bad_request` error; `Some(0)` is accepted (caller convenience)
  and normalized to NULL in the DB so the audit story stays
  honest. New migration `0005_run_retention_overrides.sql` adds
  the two nullable columns to `runs`; existing rows default to
  NULL (no override) so the upgrade is a no-op for everyone
  who doesn't opt in. See `docs/DEPLOYMENT.md` "Per-run
  retention overrides" for the operator-facing snippet.
  Closes the P2.2-deferred per-run override item.

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

- **Dense + hybrid RAG eval baselines checked in** (P10.6.2). The
  bundled `ci_offline` dataset now has three regression-gate
  baselines under `agentflow-rag/eval_baselines/ci_offline/`:
  `bm25.json` (offline, always gated on every PR), plus the new
  `dense.json` (`text-embedding-3-small`) and `hybrid.json` (RRF
  over BM25 + dense). CI gates against all three; the dense + hybrid
  steps self-skip via `if: ${{ secrets.OPENAI_API_KEY != '' }}` so
  forks without the secret stay green. A bug-fix in the
  `--compare-baseline` reader lets it accept BOTH the bare
  `EvalReport` shape (the bm25.json convention) AND the
  `{ dataset, baseline, candidate, ... }` envelope shape that
  `--output <path>` writes — pre-P10.6.2 operators had to
  hand-extract the `.baseline` field to round-trip their own
  `--output` files back through the regression gate.
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
- **`agentflow skill inspect --explain-permissions` now runs MCP
  discovery by default** (P10.9.1). Pre-P10.9.1 it was opt-in via
  `--with-mcp-discovery` because spawning every declared MCP
  server is heavy. This release flips the default and adds a
  manifest-level JSON cache at
  `~/.agentflow/cache/skill_mcp_discovery.json` (24-hour TTL,
  keyed by a stable SHA-256 of the manifest's `mcp_servers`
  section — including `name`/`command`/`args`/`env`, excluding
  `timeout_secs`/`max_concurrent_calls` which don't affect tool
  advertisements). Cache hits return in microseconds; cache
  misses show an `indicatif` spinner while the servers are
  spawned. The summary line now identifies which path was
  taken (`cache hit` / `fresh discovery (cached for next run)` /
  `forced re-discovery` / `skipped`). New `--no-mcp-discovery`
  flag opts out entirely; `--refresh-mcp-cache` busts the cache
  on demand. The old `--with-mcp-discovery` flag is kept as a
  no-op + deprecation warning so existing scripts don't break.
  13 unit tests in `commands::skill::mcp_discovery_cache::tests`
  (hash stability across env iteration order / server ordering;
  hash distinguishes argv / command / env-value changes; hash
  ignores timeout; load/save round-trip; load returns empty on
  missing file / schema mismatch / malformed JSON; TTL fresh/
  stale/unknown) + 4 hermetic CLI tests covering the
  deprecation warning, baseline (no warning when not set),
  `--no-mcp-discovery` short-circuit (with an MCP-declaring
  skill whose server script doesn't exist, so a spurious spawn
  would fail loudly), and stray-flag-without-`--explain-permissions`
  note.
- **`docs/ROADMAP_v2.md`** consolidated post-v1.0 roadmap
  (P10.19.3). Single source of truth for "what comes after v1.0
  GA", consolidating signals previously scattered across
  `RoadMap.md` Later Tracks, `TODOs.md` v1.x entries, and
  `docs/archive/PROJECT_EVALUATION_2026-05-19.md` §7. Ten themes
  (LLM/provider expansion, memory/RAG, server platform, Web UI
  debugger-scope, distributed/worker, Harness H6, plugin
  runtime WASM, perf, ops tooling, docs/contributor experience)
  with backreferences to the canonical `TODOs.md` IDs. Explicit
  v2 non-goals carry the v1 non-goals forward + add
  operator-dashboard Web UI per P10.17.1. `RoadMap.md::Later
  Tracks` gains an inline pointer at the top of the section so
  future contributors land on the consolidated view first.
- **`agentflow mcp config list --format json-envelope`** support
  (P10.11.3). The audit of all `format: String` clap fields in
  `agentflow-cli/src/main.rs` found exactly one holdout that
  didn't accept `json-envelope`: this one. The legacy `--format
  json` bare-body shape is preserved for back-compat; the new
  envelope mode wraps the same `{source, servers}` payload in
  the canonical `agentflow.cli/1` wire schema. Hermetic CLI test
  added alongside the existing json test.
- **`cargo xtask check-changelog`** subcommand (P10.18.2). Fails
  when a non-trivial source change versus a base ref (default
  `origin/main`) didn't touch `CHANGELOG.md` AND no commit body
  in the branch range carries `chore(skip-changelog)`. Trivial
  paths (`docs/`, `*.md`, `Cargo.lock` / `package-lock.json`,
  `.github/workflows/`, `tests/`, `**/fixtures/`, `*.test.ts` /
  `*.test.rs`) are excluded — that classifier is the
  single-source-of-truth pass/fail boundary, pinned by a
  dedicated unit test. Not wired into `quality.yml` today;
  available for manual + local pre-commit use. New
  `check_changelog_tests` module covers the 4 outcomes
  (only-docs / changelog-touched / skip-marker / source-but-no-
  signal) with a git fixture per test.
- **Checkpoint schema documentation** (P10.1.2) —
  `docs/CHECKPOINT_SCHEMA.md` formally documents the
  `decode_checkpoint_flow_value` warn-vs-silent fallback
  asymmetry: tagged-but-corrupt values warn loudly so a writer
  regression is debuggable; genuinely untagged legacy values
  fall back silently because spamming the operator on every
  pre-0.2 resume would be noise. The doc names the tests that
  pin each branch + the operator-facing diagnostic surface
  (`tagged ... but failed to deserialize` substring) so the
  contract stays auditable. STABILITY.md cross-references it.
- **Playwright UI e2e wired into CI** (P10.17.4). The 6 specs in
  `agentflow-ui/e2e/` (across two files: `runs-new.spec.ts` +
  `harness-sessions.spec.ts`) now run on a new
  `.github/workflows/ui-e2e.yml` workflow — `workflow_dispatch`
  + nightly schedule at 10:30 UTC, **not** in
  `quality.yml::release-gate.needs`. The pattern mirrors
  `llm-live.yml`: manual + nightly catches regressions between
  releases without the build + browser-install + flakiness tax
  on every PR. `@playwright/test` promoted from optional dev
  dep to a real `devDependencies` entry; new `npm run e2e`
  script plus a `playwright.config.ts` (Chromium-only, JUnit
  XML + HTML report on CI, trace-on-first-retry). Full
  operator + CI runbook in `agentflow-ui/e2e/README.md`. The
  CI job spins up a Postgres 16 service container, builds the
  server (release), boots it in the background with a 30s
  readiness probe against `/ui`, runs the suite, and uploads
  the `playwright-report/` HTML report as an artifact on
  failure. `workflow_dispatch` accepts an optional
  `spec_filter` input that maps to `playwright test -g
  <pattern>`.
- **Server-side `?filter=` pre-filter for events history**
  (P10.17.3). `GET /v1/runs/{id}/events/history?filter=<expr>`
  accepts the same grammar as the client-side `eventFilter.ts`
  (kind=/kind!=/kind~ + step compares joined by AND). Long runs
  no longer need to ship every event over the wire just to be
  filtered client-side. New `agentflow-server::events_filter`
  module with 21 unit tests pinning every clause shape +
  case-insensitivity + the AND-inside-value non-split rule +
  every parse-error path; new self-skipping integration tests
  in `agentflow-server/tests/events_filter_route.rs` cover
  kind-contains + after_seq+filter compose + 400-on-bad-expr +
  empty-param no-op. UI side: run-console history fetch now
  passes the operator's saved filter expression on initial
  attach; 400 responses fall back to no-filter so a malformed
  expression still loads the timeline (the inline parse error
  from the client `compileFilter` is what the operator
  actually sees and fixes).
- **Web UI preferences sync to `/v1/preferences`** (P10.17.2).
  Selected localStorage values now round-trip through the
  server's tenant-scoped preferences API so an operator's
  settings roam across browsers. Pure-helper module
  (`agentflow-ui/src/preferences.ts`) lists the syncable keys —
  tenant ids, profile selections, harness runtime kind, per-run
  event-filter expressions — and explicitly excludes the API
  token, workflow YAML drafts, harness user_input prompts, and
  workspace_root paths (security / size / machine-specific).
  React hook (`usePreferenceSync.ts`) GETs once per
  `(apiToken, tenant)` pair, debounces PUTs at 500 ms via a
  last-write-wins queue, flushes on unmount. localStorage stays
  as a fast first-paint cache. End-to-end wired today for the
  run-console tenant; the other 6 keys are mapped in the helper
  with the same 3-line pattern available for replication —
  tracked as follow-up work inside the new
  `docs/WEB_UI.md § Durable preferences (P10.17.2)` table.
  28 PASS in `preferences.test.ts` (bun-driven; same node-tsx
  pattern as `eventFilter.test.ts`); `npx tsc --noEmit` clean.
- **`agentflow harness replay <session_id>`** subcommand
  (P10.10.2). Re-streams a persisted JSONL session log with
  time-paced output (sleeps between events based on their
  original `ts` deltas). Useful for debugging long-finished
  sessions where the *pacing* of events carries diagnostic value
  — e.g. spotting a tool call that fired right before a stall.
  `--speed 1x` (default) honours the original timing; `2x` /
  `0.5x` scale it; `inf` / `instant` skip all sleeps (== `resume`
  but routed through the per-event formatter). `--from-seq` /
  `--to-seq` clip the visible window; `--filter-kind` (repeatable)
  acts as an OR include-list over the `kind` discriminator.
  `--output {text, stream-json}` — `json` / `json-envelope` are
  rejected because replay is open-ended (mirrors the
  `workflow logs --follow` rejection). The 1-hour sleep cap
  prevents an overnight idle gap from hanging the replay (run
  with `--speed inf` if you really want the original timing).
- **`agentflow memory prune --layer {preference,entity_facts}
  --db <path> --older-than <duration>`** subcommand (P10.7.1)
  wires the existing trait surface
  (`PreferenceStore::prune_older_than`,
  `EntityFactStore::prune_invalidated`) to a CLI front-end so
  operators can drop stale memory rows from the command line.
  `--older-than` accepts `<integer><unit>` where unit ∈
  `{s, m, h, d, w, y}` — bare integers are rejected so a typo
  (`--older-than 30` instead of `--older-than 30d`) can't
  silently mean "30 seconds". `--format text` (coloured ✓ line
  with row count) or `--format json-envelope` (canonical
  `agentflow.cli/1` wrapper carrying `{layer, db, older_than,
  older_than_seconds, removed_rows}`). The `entity_facts` path
  is invalidation-bounded — active facts are never touched, even
  with `--older-than 0s`. Session + semantic layers are out of
  scope for this slice because they expose per-session clear
  rather than retention-based prune. Defaults `--db` to
  `~/.agentflow/memory.db` matching the agent-runtime
  convention. 6 unit tests + 5 hermetic integration tests cover
  the parser, layer routing, missing-db error, unsupported-layer
  rejection, and the end-to-end round-trip against a real
  on-disk SQLite file seeded via the public memory-crate API.
- **`agentflow workflow run --server` flag validation** (P10.11.4):
  closes the silent-drop class of bug for the local-only flag set
  by rejecting up front with a single-line actionable error when
  any of `--model`, `--execution-mode` (non-default),
  `--max-concurrency` (non-default), `--run-dir`, `--watch`,
  `--output`, `--input`, `--dry-run`, `--timeout` (non-default),
  or `--max-retries` (non-zero) is combined with `--server`. Two
  categories: **always-local** (filesystem + in-process flow:
  `--run-dir` / `--output` / `--watch` / `--dry-run`) each name a
  concrete server-side alternative (e.g. `--watch` points at
  `agentflow workflow logs <run_id> --follow`); **future API
  addition** (server-side execution knobs the wire format could
  accept but doesn't today: `--model` / `--execution-mode` /
  `--max-concurrency` / `--input` / `--timeout` / `--max-retries`)
  each name P10.11.4 so curious operators can find the follow-up
  work. Defaults pass through silently — the validator only fires
  when the operator explicitly overrode a flag. 13 unit tests + 11
  hermetic CLI tests cover every guard + the baseline-passes path.
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

### Changed (stability)

- **`agentflow-mcp::server` promoted from Experimental to Beta**
  (P10.5.2). The closed method set is now pinned:
  `initialize` / `notifications/initialized` / `tools/list` /
  `tools/call`. New methods may be added in minor releases; the
  existing four stay wire-stable. New public surface includes
  `MCPServer::handle_request` (the single request → response
  entry point, now `pub` so non-stdio transports can drive it)
  and `STABLE_PROTOCOL_VERSION = "2024-11-05"` (bumping this is
  the explicit signal that the wire contract changed). Backed by
  6 fixture-driven compat tests + 2 invariant tests in
  `agentflow-mcp/tests/server_contracts.rs`. The fixture format
  pins required fields + exact values + error envelope shapes but
  tolerates additive fields, matching the Beta promise. See
  `docs/STABILITY.md` for the full contract and fixture-ownership
  row.

### Removed

- **`agentflow-viz` crate** (P10.13.1). Removed alongside the
  `/v1/runs/{id}/graph` REST endpoint, the `agentflow workflow
  graph` CLI subcommand, the `RunGraphResponse` shape, and the
  Mermaid `<pre>` block in the Web UI. An honest audit revealed
  the "DAG visualisation" surface was a button grid of node
  status badges plus the raw Mermaid markdown text in a code
  block — no SVG, no spatial layout, no edges. The data-plumbing
  cost (an entire workspace crate + a beta REST route + a CLI
  subcommand + the UI fetch path) was disproportionate to the
  rendering value. The UI's node-status grid is now derived
  entirely from event payloads, which was already the source of
  truth for execution state. A future RFC may revisit graphical
  DAG / agent topology rendering as an additive feature
  (e.g. mounting mermaid.js to render `agentflow workflow
  validate --output mermaid` as SVG client-side); see
  `docs/ROADMAP_v2.md` Theme D for the decision rationale.
  Stability impact: `/v1/runs/{id}/graph` was listed as Beta in
  `docs/STABILITY.md`; the row was deleted with a P10.13.1
  cross-reference so anyone hitting the old endpoint gets a
  pointer to the rationale.

- **`agentflow-mcp::client_old`** and the legacy `transport` module
  it depended on (P10.5.1). Both were `#[doc(hidden)]` and had no
  external callers in the workspace; deleting them removes ~330
  lines of dead code. The current `transport_new` module is renamed
  to `transport` so the post-cleanup name is internally consistent.
  A `#[deprecated]` `pub use transport as transport_new;` re-export
  preserves the old import path for any 3rd-party caller through
  the transition window — they get a deprecation warning instead of
  a hard break. A compat unit test pins the alias's type identity
  so the re-export can't silently degrade. agentflow-mcp is below
  the stability tier line per `docs/STABILITY.md`, so this rename
  is in scope.

- **6 dead `agentflow-llm/config/models/*.yml` files** (F-A7-3,
  `5578743`). The vendor_configs split was never read by the
  runtime registry. 4 misdirecting docs updated to point at the
  real source (`templates/default_models.yml`).

### Docs / conventions

- **WASM plugin runtime 1-pager** (P10.19.1,
  `docs/WASM_PLUGIN_EVALUATION.md`). The narrowed
  wasmtime-vs-wasmer-vs-extism comparison concludes that
  wasmtime + WIT + WASI 0.2 is the right runtime *if* we adopt
  WASM, and decides to **push the adoption itself to v2.0**.
  Subprocess JSON-RPC is mature and the 50-200 ms subprocess
  cold start is dominated by the first LLM call's TCP
  handshake in any realistic workflow — the ~6-8 person-week
  pre-GA investment doesn't fix any current friction. The
  `PluginRuntime::Wasm` enum variant stays in `manifest.rs` as
  a forward-compatible reservation; v2 wires the real host
  when at least one of the documented re-evaluation triggers
  fires (latency complaint, polyglot demand, distribution
  complaint, or peer-project precedent). Closes the
  P10.19.1 HIGH pre-GA item from the v1.0 RC backlog.

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
