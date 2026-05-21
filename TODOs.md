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

- DONE P10.0.1 Production deployment dress-rehearsal walkthrough
  - Landed as `scripts/production_dress_rehearsal/` — companion
    to the P10.0.5 doctor-smoke; reuses the same Apple-container
    pattern but bundles both `agentflow` and `agentflow-server`
    binaries and walks all 6 steps + 4 acceptance gates inside a
    fresh `ubuntu:24.04` container.
  - **The TODO's deliverable**: "Cross off each step + record
    `agentflow doctor --profile production --backup-check --format
    json` exit code." → **doctor exit code: 0, status: ok**,
    captured in the checked-in `last-run.json` + `last-run.log`
    fixtures. The 6-step matrix:
    1. `AGENTFLOW_SECURITY_PROFILE=production` env: **PASS**.
    2. `AGENTFLOW_API_TOKEN` via `openssl rand -hex 32`
       (64-char): **PASS**.
    3. 5 storage dirs pre-provisioned at
       `/var/lib/agentflow/{runs,traces,marketplace-cache,skills,
       plugins}` via `install -d -m 0750`: **PASS**, all reachable
       + writable.
    4. `DATABASE_URL` exported at `db.invalid:5432`:
       **PASS-NOTED** — env wiring works; no Postgres sidecar in
       the single-container rehearsal (validated host-side via
       docker compose, see README).
    5. `agentflow doctor --profile production --backup-check
       --format json`: **PASS**, exit 0, status `ok`. This is the
       canonical TODO deliverable.
    6. docker compose smoke: **SKIP** — explicitly optional in
       the upstream checklist and host-side.
    Acceptance gates:
    - AG1 (doctor exits 0): **PASS** (same evidence as step 5).
    - AG2 (`serve --check --security-profile production`):
      **SKIP-NEEDS-POSTGRES**. Despite the source-code comment
      claiming `serve --check` is "non-binding readiness
      diagnostic which does not require Postgres", the
      implementation does attempt a real DB connection. The
      rehearsal flags this as a stale-comment follow-up (see
      `README.md::Filed follow-ups`); not a release blocker
      because the host-side compose flow exercises the same path
      with a real DB.
    - AG3 (`/health/ready` returns 200): **SKIP** (host-side).
    - AG4 (authenticated `POST /v1/runs`): **SKIP** (host-side).
  - **Layout**:
    - `Containerfile` — two-stage: rust:1-slim-bookworm builds
      both binaries → ubuntu:24.04 carries them + ca-certificates
      + openssl.
    - `inside_container.sh` — the rehearsal script that walks
      all 10 step+gate items and emits a JSON summary.
    - `run.sh` — host-side driver: builds the image (if not
      cached, ~12-18 min first run) + pipes the rehearsal script
      to `bash -s -i` inside a fresh container + extracts the
      JSON summary.
    - `last-run.{log,json}` — checked-in canonical-passing
      fixtures.
    - `README.md` — usage, step matrix table, host-side
      follow-up commands for steps 6 / AG3 / AG4, filed
      follow-ups.
  - **Discoveries** during the rehearsal:
    - `serve --check` requires Postgres despite a stale source
      comment claiming otherwise. Filed as a future-PR
      bookkeeping item in the README.
    - `agentflow serve` shells out to a separate
      `agentflow-server` binary (not bundled with the CLI by
      default). The rehearsal image bundles both; the doctor-
      smoke image deliberately doesn't, to keep the "minimal
      first-user install" baseline honest.
    - Apple `container run` requires `-i` (interactive flag) to
      pipe a script via stdin to a `bash -s` entrypoint; without
      it, stdin is detached and the script silently no-ops.
      Documented inline in `run.sh`.
  - **Verification**: end-to-end run on apple-aarch64 via Apple
    `container 0.12.3` produces the checked-in `last-run.json`
    with exit 0 + all in-container steps passing. Re-runs via
    cached image take ~10 s (no rebuild needed).
  - **Wiring deliberation**: like P10.0.5's doctor smoke, NOT
    wired into `quality.yml`. 12-18 min build cost wrong fit for
    per-PR gating; manual pre-release operator step. Paired with
    P10.0.5 in `docs/RELEASE_NOTES_v1.0.0-rc.1.md`'s pre-tag
    checklist.
- DONE P10.0.2 `cargo publish --dry-run` for all publishable crates
  - Order: `agentflow-core` → `agentflow-tools` → `agentflow-tracing`
    → `agentflow-llm` → `agentflow-nodes` → `agentflow-mcp` →
    `agentflow-memory` → `agentflow-rag` → `agentflow-agents` →
    `agentflow-skills` → `agentflow-harness` →
    `agentflow-db` → `agentflow-server` → `agentflow-worker` →
    `agentflow-cli`. xtask, ui not published.
  - **Required fix landed**: every `[dependencies]` path-dep on a
    workspace-internal crate now carries an explicit
    `version = "X.Y"` next to `path = "../..."`. Cargo's publish
    flow rejects bare path-only deps with
    `all dependencies must have a version requirement specified
    when publishing`. Fix covers 11 of the 15 publishable crates
    (`agentflow-tracing` / `agentflow-llm` / `agentflow-nodes` /
    `agentflow-mcp` / `agentflow-memory` / `agentflow-agents` /
    `agentflow-skills` / `agentflow-harness` / `agentflow-server` /
    `agentflow-worker` / `agentflow-cli`). Dev-dependencies don't
    need versions for publish (cargo ignores them in published
    manifests), so the `[dev-dependencies]` path-deps in
    `agentflow-harness` / `agentflow-cli` are left as-is.
  - **Cargo.lock yank fix**: `cargo update -p slab` bumped the
    transitively-pinned `slab v0.4.10` (yanked from crates.io) up
    to `v0.4.12`. The dry-run warning is now gone.
  - **Dry-run scope (4 of 15 crates pass end-to-end locally)**:
    `agentflow-core`, `agentflow-tools`, `agentflow-rag`, and
    `agentflow-db` have no workspace-internal `[dependencies]`,
    so their `cargo publish --dry-run` resolves cleanly and
    packages a valid `.crate` tarball. Confirmed locally.
  - **Cargo limitation (11 of 15 crates can't dry-run locally)**:
    the remaining 11 crates package cleanly (the `Packaging
    <crate> v<ver>` step succeeds for every one) but fail at the
    upload-prep step's `Updating crates.io index` because their
    workspace-internal deps aren't on crates.io yet — either the
    crate has never been published (e.g. `no matching package
    named 'agentflow-tracing' found`) or the on-registry version
    predates the local bump (e.g. `failed to select a version for
    the requirement 'agentflow-core = "^0.2"'; candidate versions
    found which didn't match: 0.1.2, 0.1.1`). This is the
    well-known monorepo-publish constraint:
    `cargo publish --dry-run` consults the real registry for
    dep resolution; neither `--no-verify`, `--offline`, nor
    `[patch.crates-io]` bypasses the upload-prep validation.
  - **Real-environment gap → folded into P10.0.3**: a true
    end-to-end dry-run of the full 15-crate chain requires the
    deps to be live on crates.io. The natural shape is the
    sequential publish flow of P10.0.3 — dry-run a crate, real
    publish if green, wait for the index refresh, dry-run the
    next dependent. P10.0.3 already covers this, so no separate
    follow-up is opened.
  - **Cosmetic warning surfaced (pre-GA, not blocking publish)**:
    14 of 15 manifests still emit `manifest has no documentation,
    homepage or repository`. Only `agentflow-rag` has
    `repository = "https://github.com/agentflow/agentflow"`. Adding
    the three metadata fields workspace-wide is tracked as
    `P10.0.2-FU1` below. Not a blocker — crates.io accepts publish
    with the warning.

- DONE P10.0.2-FU1 (Low — pre-GA) Workspace package metadata via
  `[workspace.package]`
  - Landed via Rust 1.64+ workspace inheritance (matches the
    "cleanest fix" path the original TODO suggested). New
    `[workspace.package]` block in root `Cargo.toml` pins the
    five shared fields once:
    - `edition = "2024"` (was duplicated 15×)
    - `license = "MIT"` (was duplicated 15×)
    - `authors = ["yuxuetr", "AgentFlow Contributors"]` (unifies
      the two prior styles)
    - `repository = "https://github.com/yuxuetr/agentflow"` (NEW
      — clears the dry-run warning workspace-wide)
    - `homepage = "https://github.com/yuxuetr/agentflow"` (NEW)
  - Every one of the 15 publishable members opts in via
    `<field>.workspace = true` on the five fields above. Per-crate
    `name` / `version` / `description` / `keywords` / `categories`
    stay in each member's `[package]` table — those are the fields
    that genuinely differ per crate.
  - **Three pre-existing `repository` values cleaned up at the
    same time**: `agentflow-rag` and `agentflow-cli` both pointed
    at `https://github.com/agentflow/agentflow` (a 404 — the
    `agentflow` org doesn't exist); `agentflow-mcp` correctly
    pointed at the canonical URL. All three replaced with
    `repository.workspace = true` so they inherit the single
    canonical value. A future repo rename / org move is a
    one-line change to the workspace block instead of a 15-file
    `sed`.
  - **`xtask` deliberately doesn't opt in** — it's `publish =
    false` and internal-only; pulling it into the shared metadata
    table would be misleading.
  - **Verification**: every one of the 15 publishable crates now
    dry-runs without the `manifest has no documentation, homepage
    or repository` warning. The 4 leaf crates (`agentflow-core`,
    `agentflow-tools`, `agentflow-rag`, `agentflow-db`) confirmed
    end-to-end with `cargo publish --dry-run --allow-dirty -p
    <crate>`; the other 11 confirmed via `--no-verify --allow-dirty`
    + grep (the dep-resolution failure documented in P10.0.2
    still kicks in afterward, but the manifest warning is
    suppressed). `cargo clippy --workspace --tests -- -D warnings`
    clean; `cargo build --workspace` clean.
  - **Out of scope** (genuinely per-crate, not shared workspace
    data): adding `documentation = "https://docs.rs/<crate>"` per
    member. crates.io auto-discovers the docs.rs link from the
    package name anyway, so the explicit field is a cosmetic-
    only nicety; deferred until / if it surfaces an operator
    complaint.
- DONE P10.0.3 Tag `v1.0.0-rc.1`
  - Annotated local tag created at the commit that closes the
    last active P10 item (`9dc4c99` —
    `feat(memory): age-based encryption-at-rest for preferences
    (P10.7.2)`). Tag message summarises the four-layer maturity
    arc + cross-references `docs/RELEASE_NOTES_v1.0.0-rc.1.md`'s
    What's New / Breaking Changes / Known Issues sections, which
    were filled in at tag prep time.
  - **Crate-version posture (documented in the release notes)**:
    the workspace's 15 publishable crates stay on their
    independent pre-1.0 SemVer trajectories (`agentflow-core
    0.2.0`, `agentflow-rag 0.3.0-alpha`, etc.). The
    `v1.0.0-rc.1` git tag is a **project-milestone marker** —
    "the operator-facing stability surfaces are RC-ready" — not
    a coordinated crate version bump. Coordinated 1.0 crate
    publishing is a separate follow-up after RC-period feedback;
    keeps the tag cut from coupling to a 15-manifest version
    bump that would warrant its own PR.
  - **Push semantics**: pushing the tag (`git push origin
    v1.0.0-rc.1`) fires the `.github/workflows/release.yml`
    pipeline landed in P10.0.4 — 4-target CLI binaries +
    multi-arch GHCR image + GitHub Release with
    `SHA256SUMS.txt`. End-to-end ~25-35 min. Admin then flips
    GHCR package visibility from private (default) to public
    per the `docs/RELEASE_CHECKLIST.md` §10 first-push
    prerequisites.
  - **Reversibility**: the local tag is reversible
    (`git tag -d v1.0.0-rc.1`) until pushed. The push is the
    one-way step.
- DONE P10.0.4 GitHub Release artifact + docker image push
  - Landed `.github/workflows/release.yml` (3-job pipeline) +
    `scripts/release_dry_run/` (local rehearsal of the build legs
    without push) + RELEASE_CHECKLIST.md §10 documenting the
    flow.
  - **Trigger surface**:
    - `push` of any `v[0-9]+.[0-9]+.[0-9]+*` tag (so
      `v1.0.0-rc.1`, `v1.0.0`, `v0.3.0-alpha` all match) fires
      the full pipeline including push + release publish.
    - `workflow_dispatch` accepts `ref` (tag or branch) +
      `dry_run` (default `true`). Dispatch path builds the
      binaries + multi-arch image but skips the GHCR push and
      the release publish.
  - **3 jobs**:
    1. `build-binaries` — 4-row matrix (linux x86_64 +
       aarch64, macOS Intel + Apple Silicon). Each row builds
       `cargo build --release -p agentflow-cli --bin agentflow`,
       tars to `agentflow-<target>.tar.gz`, sidecar `.sha256`,
       uploads via `actions/upload-artifact@v4`.
       `fail-fast: false` so a single platform's regression
       doesn't kill the other 3.
    2. `build-docker` — `docker/setup-qemu-action@v3` +
       `docker/setup-buildx-action@v3`, then
       `docker/build-push-action@v5` against the root
       `Dockerfile` for `linux/amd64,linux/arm64`. Pushes to
       `ghcr.io/<owner-lower>/agentflow-server:<tag>`; also
       pushes `:latest` when the tag has no `-` (so pre-release
       tags can't accidentally promote latest). OCI labels
       carry the source commit + tag + license for
       traceability. `permissions: packages: write` declared
       at job scope.
    3. `publish-release` — needs both upstream jobs. Downloads
       all artifacts, flattens to a single `release/` dir,
       aggregates per-archive `.sha256` files into one
       `SHA256SUMS.txt`, uploads via
       `softprops/action-gh-release@v2` with
       `generate_release_notes: true`,
       `fail_on_unmatched_files: true`. `prerelease` flag
       auto-derives from the `-` in the tag.
       `permissions: contents: write` declared at job scope.
  - **First-push prerequisites** documented in
    RELEASE_CHECKLIST.md §10:
    - GHCR package must be flipped to `public` visibility after
      the first successful push (default is `private`).
    - `ubuntu-24.04-arm` runner is GA since 2025 on public
      repos and most paid plans.
    - `secrets.GITHUB_TOKEN` covers both `packages: write`
      (GHCR push) and `contents: write` (release publish) when
      declared at job scope.
  - **`scripts/release_dry_run/run.sh`** walks the same build
    legs locally without pushing. Catches Dockerfile / feature-
    flag / dep-graph regressions before the tag cut. Uses
    `cargo metadata` to resolve the real `target_directory`
    (works with the AgentFlow dev convention of
    `~/.cargo/config.toml::build.target-dir =
    /Users/hal/.target`). Apple `container` doesn't expose
    `buildx`, so the docker leg falls back to single-arch local
    build on the default runtime; pass
    `DOCTOR_SMOKE_RUNTIME=docker` to invoke the full multi-arch
    buildx leg.
  - **Verification (local dry-run on apple-aarch64)**:
    - `cargo build --release -p agentflow-cli --bin agentflow`:
      40 s (warm cache). Binary produced at the cargo metadata
      target_directory; `--version` smoke runs clean.
    - Apple `container build -f Dockerfile`: 6 min cold (workspace
      compile in the rust:1-bookworm builder stage), tags
      `agentflow-server:dry-run`. Confirms the root Dockerfile
      still builds the server binary end-to-end.
  - **Wiring deliberation**: workflow only runs on real tag
    pushes (or explicit dispatch). NOT wired into `quality.yml`
    — same rationale as the doctor/dress-rehearsal scripts,
    build cost too high for per-PR gating.
  - **Real-cut sequence** (after P10.0.3's tag decision):
    1. `git tag v1.0.0-rc.1`
    2. `git push origin v1.0.0-rc.1`
    3. workflow auto-fires, ~25-35 min end-to-end.
    4. Admin flips GHCR package visibility to public.
    5. Release page shows 4 tarballs + SHA256SUMS.txt + auto-
       generated commit list.
- DONE P10.0.5 Fresh-VM doctor smoke
  - Landed as a reproducible smoke under `scripts/doctor_smoke/`
    (Containerfile + driver script + checked-in `last-run.json`
    fixture + README). Drives Apple `container` (or Docker via
    `DOCTOR_SMOKE_RUNTIME=docker`) end-to-end: multi-stage build
    against `rust:1-slim-bookworm` → fresh `ubuntu:24.04` smoke
    image with zero `~/.agentflow/` state → canonical
    `agentflow doctor --profile production --backup-check --format
    json` invocation as CMD.
  - **The "expect exit code 0 (or document why not)" half of the
    TODO is honestly closed as "document why not"**: production-
    profile doctor on a fresh VM produces `status: fail`
    (exit code 2) because every default `~/.agentflow/*` dir
    (`runs`, `traces`, `marketplace/cache`, `skills`, `plugins`)
    reports `exists: false`, and production-profile treats
    missing dirs as fail (vs. warning on `dev`/`local`). This is
    by design — `agentflow-cli/src/commands/doctor.rs` Lines
    518-525 explicitly promote `--profile production` missing-dir
    checks to `DoctorStatus::Fail`. The doctor binary itself runs
    cleanly end-to-end; the exit code is the *expected first-run
    signal* on a fresh box.
  - **Why a checked-in fixture (last-run.json) rather than an
    assertion?** It's documentation + a diff target, not a strict
    oracle. The fixture's `disk.run_dir.error` string carries a
    fixed message today; future localisation or timestamp
    additions would false-positive against a brittle assertion.
    The README + the driver's `jq`-summary keep the workflow
    honest without locking in incidental formatting details.
  - **Re-running**: `scripts/doctor_smoke/run.sh`. ~10-15 min on
    first run (Rust toolchain + workspace compile); cache hits
    bring re-runs down to ~30 s. The driver only surfaces exit
    codes `>2` as its own non-zero (those represent the binary
    *crashing*, not the doctor's profile semantics).
  - **Wiring deliberation**: not added to `quality.yml` — the
    10-15 min build cost is wrong fit for per-PR gating. Manual
    pre-release operator step, called out in the README +
    `docs/RELEASE_NOTES_v1.0.0-rc.1.md` checklist step 5.

### P10.1 — agentflow-core (A — already strong, micro-polish only)

- DONE P10.1.1 (Stretch) Benchmark hot-path scheduler / FlowValue
  decode / checkpoint roundtrip
  - The scheduler half was already covered by
    `agentflow-core/benches/scheduler.rs` (DAG mechanics: linear +
    fan-out across 10/100/1000 nodes, serial vs concurrent). The
    two remaining hot paths called out in the TODO landed as a new
    `agentflow-core/benches/hot_paths.rs` criterion suite with two
    groups + 9 bench points total:
    - `flowvalue_decode/json/{tiny,medium,large}` — `serde_json::
      from_value::<FlowValue>` over the P0.2 tagged-enum wire shape
      across realistic payload sizes (5-field object / 50-item
      array / 500-item nested doc).
    - `flowvalue_decode/{file,url}/metadata_only` — the
      `File` / `Url` variants. Tiny because they only carry the
      path/url + mime tag, but worth a separate row so the
      bench-gate flags any regression that disproportionately
      affects the non-Json variants.
    - `checkpoint_roundtrip/save_and_load/{10,100}` — full
      `CheckpointManager::save_checkpoint_with_status` +
      `load_latest_checkpoint` cycle through a `tempfile::TempDir`.
      `iter_batched` keeps the temp-dir setup out of the measured
      body. `Throughput::Elements(node_count)` reports the
      per-node-state-entry rate.
    - `checkpoint_roundtrip/decode/{10,100}` — bench-isolated
      `serde_json::from_str::<Checkpoint>` against a pre-serialised
      payload, no fs cost. Isolates the deserialiser from the
      save_and_load wall-clock so a deserializer-only regression
      can be triangulated.
  - **Naming convention pin**: `BenchmarkId::new(<variant>,
    <param>)` produces a criterion path
    `<group>/<variant>/<param>` on disk. Slashes inside either
    component get sanitised to `_`, so the bench deliberately
    uses pair-form ids (`BenchmarkId::new("json", "tiny")`) rather
    than `BenchmarkId::from_parameter("json/tiny")` which would
    write to `flowvalue_decode/json_tiny/` and miss the
    bench-gate's exact-match lookup. Documented inline in the
    bench file so the next time someone adds a row they don't
    repeat the discovery.
  - **Baseline numbers** captured on apple-aarch64 with
    `--warm-up-time 1 --measurement-time 3 --sample-size 20`:
    `flowvalue_decode/json/tiny` ≈ 650 ns, `medium` ≈ 21.6 µs,
    `large` ≈ 416 µs; `flowvalue_decode/{file,url}` ≈ 300 ns;
    `checkpoint_roundtrip/save_and_load/10` ≈ 7.86 ms, `/100` ≈
    9.32 ms (fs-dominated, so the per-node delta is small);
    `checkpoint_roundtrip/decode/10` ≈ 162 µs, `/100` ≈ 1.66 ms.
    All 9 rows land in
    `benches/baselines/apple-m2-max.json::"agentflow-core/hot_paths"`.
  - **TODO investigation half**: the original P10.1.1 also asked
    to "look for any 1.10× regressions accumulated during P3.3
    envelope work". Investigated: P3.3 was the CLI `--format
    json-envelope` wave (`a28816c` + 10+ follow-ups) — it touches
    `agentflow-cli` only, with zero changes to `agentflow-core::
    {value,checkpoint,flow}`. No regression-investigation target
    surfaced. The newly-captured baseline locks in the post-N8
    numbers so any *future* regression in either hot path now
    trips `bench-gate`'s 1.25× default threshold. The TODO's
    looser 1.10× note is operator-side intuition for "review the
    PR carefully" — `cargo test` wall-clock variance on the
    save_and_load rows alone runs ±5% on a hot laptop, so a 1.10×
    hard gate would false-positive on day-to-day noise; the
    default 1.25× is the right setting.
  - **CI wiring**: `.github/workflows/bench.yml` gains a new
    `cargo bench (hot_paths)` step right after the existing
    `cargo bench (scheduler)` step. Per-bench step isolation
    keeps a panic in one from masking the rest. Job timeout
    budget (30 min) is comfortably unchanged — the 6th bench
    fits in ~1 minute.
  - `benches/baselines/README.md` table extended with the new
    row; `agentflow-core/Cargo.toml` gains a second `[[bench]]
    name = "hot_paths" harness = false` block alongside the
    existing scheduler block.
  - **Verification**: `cargo bench -p agentflow-core --bench
    hot_paths --no-run` compiles clean. `cargo clippy -p
    agentflow-core --benches -- -D warnings` clean. End-to-end
    `cargo xtask bench-gate --allow-missing` shows
    `19 compared, 0 regression(s)` (10 from `node_latency` +
    9 from this commit's `hot_paths`).
- DONE P10.1.2 (Stretch) Document `decode_checkpoint_flow_value`'s
  warn-vs-silent fallback in `docs/CHECKPOINT_SCHEMA.md`
  - The doc file didn't exist; created it as the canonical
    checkpoint-schema reference. Documents the on-disk tagged
    `FlowValue` shape (json/file/url variants), the reader
    contract (three input categories — tagged-clean,
    tagged-corrupt, untagged), and the operator-facing
    asymmetry between the two fallback paths. Names the tests
    that pin each branch
    (`malformed_tagged_checkpoint_value_falls_back_to_json`,
    `legacy_untagged_checkpoint_values_decode_as_json`,
    `legacy_raw_json_checkpoint_values_read_as_json_flow_values`)
    and the operator-facing `tagged ... but failed to
    deserialize` substring so a debugger can grep stderr.
    `docs/STABILITY.md` cross-references the new file from
    its existing Checkpoint state row.

### P10.2 — agentflow-nodes (A-)

No active gaps from the evaluation. Future opportunities:

- DONE P10.2.1 (Stretch) Add a node-level latency bench for the
  built-in nodes
  - Landed as a new criterion bench file
    `agentflow-nodes/benches/node_latency.rs` plus a 10-row
    baseline section added to `benches/baselines/apple-m2-max.json`
    under the `agentflow-nodes/node_latency` key. The bench is
    wired into the existing `bench-gate` CI workflow with the same
    fast flags the four prior benches use (`--warm-up-time 1
    --measurement-time 1 --sample-size 10`).
  - **Scope decision** — only the *pure-compute* built-in nodes
    are benched. The other 12+ nodes (`llm`, `http`, `mcp`, `rag`,
    `arxiv`, `markmap`, `asr`, `tts`, image variants) depend on
    real external services (LLM providers, HTTP endpoints, MCP
    server spawns), which makes them poor regression-detection
    targets — latency is dominated by external round-trip, not
    by the AgentFlow code path. Benching them properly belongs
    in a separate nightly suite with mocks / fixtures, tracked
    as a future follow-up if real demand surfaces.
  - **Coverage**: 10 bench points across 3 node families.
    - `template/render/{small,medium,large}` — Tera rendering of
      a synthetic template with for-loop + filter + condition,
      across 4 / 32 / 256 loop iterations. The TODO explicitly
      called out template-render regressions as the headline
      target.
    - `conditional/evaluate/{exists,equals,greater_than}` —
      pattern-match dispatch + HashMap lookup. Catches an O(N)
      sneak into the eval path.
    - `file/{read,write}/{1k,64k}` — `tokio::fs` round-trip
      through a `tempfile::TempDir`. Local-FS-dependent, but the
      relative ratio across runs is what bench-gate watches.
  - **Wiring**:
    - `agentflow-nodes/Cargo.toml` gains
      `criterion = { version = "0.5", features = ["async_tokio"] }`
      under `[dev-dependencies]` plus a
      `[[bench]] name = "node_latency" harness = false
      required-features = ["conditional"]` block. The
      required-features pin auto-enables the gated module so the
      bench compiles deterministically.
    - `.github/workflows/bench.yml` gains a new `cargo bench
      (node_latency)` step + a path-trigger entry for
      `agentflow-nodes/**` (so a node-impl PR re-runs the gate).
      The job's runtime budget is unchanged — the 5th bench fits
      comfortably under the 30-minute timeout.
    - `benches/baselines/README.md` table extended with the new
      row + the `--features conditional` capture flag.
  - **Baseline numbers** captured locally on apple-aarch64 with
    `--warm-up-time 1 --measurement-time 3 --sample-size 20`:
    template render varies from 39 µs (small) → 415 µs (large);
    conditional dispatch is ~8 µs across all 3 variants; file
    read/write is 20–65 µs across the size matrix. The default
    `bench-gate` 1.25× threshold absorbs day-to-day variance
    cleanly — `cargo xtask bench-gate --allow-missing` confirmed
    `10 compared, 0 regression(s)` on the first run after the
    baseline was committed.
  - **Verification**: `cargo bench -p agentflow-nodes --bench
    node_latency --features conditional --no-run` compiles
    clean. `cargo clippy -p agentflow-nodes --benches --features
    conditional -- -D warnings` clean. End-to-end
    bench-gate-against-fresh-criterion-run passes for the new
    rows.
  - **Known noise**: `TemplateNode::execute` and
    `ConditionalNode::execute` both print debug lines (e.g.
    `📝 Rendering template for node 'bench'`) on every call.
    That's pre-existing production behaviour; the baseline
    numbers include the print cost. A separate follow-up to
    gate those prints behind a verbose flag would tighten the
    `small`-end variance, but is out of scope here.

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

- DONE P10.3.2 (Medium — v1.x) Promote DashScope / DeepSeek /
  MiniMax to dedicated provider modules (R16)
  - Resolved as a 1-pager (`docs/LLM_PROVIDER_MODULE_PROMOTION.md`)
    pinning per-vendor promotion triggers, not as code. The
    TODO was trigger-gated ("until vendor introduces wire-
    format divergence, keep shared adapter") so closing it
    means making the gate empirically verifiable, not extracting
    any of the three vendors prematurely. Same posture as
    P10.19.1 (WASM) and P10.10.1 (H6).
  - **Six concrete triggers** documented (any one tips the
    scale for that specific vendor): tool-call shape divergence
    (caught by the `cross_provider_tool_call_paths_*` invariant),
    multimodal-shape divergence (caught by
    `cross_provider_multimodal_paths_*`), streaming-protocol
    divergence (caught by `cross_provider_streaming_paths_*`),
    auth/endpoint topology divergence (HMAC-SHA1, per-request
    OAuth, etc.), vendor-specific feature with no upstream
    OpenAI mapping (e.g. DeepSeek `reasoning_content`), and
    operator-side issue request.
  - **None has fired** as of 2026-05-20. The nightly
    cross-provider live suite passes for all 4 OpenAI-compat
    vendors. The doc also captures the mechanical migration
    steps for the day one trigger does fire (new
    `agentflow-llm/src/providers/<v>.rs`, dispatch update in
    `create_provider`, vendor-specific tests, doc updates).
  - `docs/ROADMAP_v2.md` Theme A updated to mark this closed
    with a pointer to the criteria doc. Future P11.x extraction
    opens a fresh TODO referencing the criteria; no formal RFC
    needed for the peel-off itself once a trigger fires.

- DONE P10.3.3 (Medium — v1.x) Provider-specific tokenizers
  (foundation slice)
  - Landed the trait surface + accuracy improvement for the
    OpenAI family, not the workspace-wide rip-out of the
    heuristic. New module `agentflow-llm/src/tokenizer.rs`
    ships `TokenCounter` trait, `TiktokenCounter` (BPE via
    `tiktoken-rs` 0.6 — cl100k_base, o200k_base, p50k_base,
    r50k_base), `HeuristicCounter` (preserves the existing
    `len / 4` fallback), and `counter_for_model(model_id)` +
    `count_tokens_for_model(model_id, text)` factories.
  - Coverage matrix documented in the module-doc table:
    - **Exact (tiktoken)**: `gpt-3.5-*`, `gpt-4*`, `gpt-4o*`,
      `o1*`, `o3*`, `gpt-5*` → BPE-accurate.
    - **Close (cl100k_base used as approximation)**: Moonshot
      Kimi (`kimi-*`, `moonshot-v*`), DeepSeek (`deepseek-*`),
      GLM (`glm-*`, `chatglm*`), DashScope Qwen (`qwen*`),
      MiniMax (`abab*`, `minimax-*`), StepFun (`step-*`). The
      per-vendor accuracy gap is documented inline (5-15%
      depending on family).
    - **Heuristic fallback**: Anthropic (`claude-*`), Google
      (`gemini-*`, `models/gemini*`), and unknown model ids.
      Provider responses still report exact counts for
      post-call accounting, so the precision gap only affects
      pre-call budget enforcement.
  - 13 hermetic unit tests cover BPE counts against known
    inputs (cl100k_base + o200k_base), heuristic backward
    compat, model-name routing for every documented family,
    case-insensitivity, free-function round-trip, and the
    error path when an unknown encoding name is requested.
  - **Not in scope** (follow-up TODO `P10.3.3-FU1`): rip out
    the `content.len() / 4` heuristic at every
    `agentflow-memory::Message::new` site and route through
    `count_tokens_for_model`. Doing that in one shot would
    ripple through 50+ test sites and obscure the accuracy
    improvement, so the trait surface lands first.
  - Adds `tiktoken-rs = "0.6"` dep to `agentflow-llm`.

- DONE P10.3.3-FU1 (Low — v1.x) Wire `count_tokens_for_model`
  into `agentflow-memory::Message::new`
  - Resolved without the 50-site rip-out. New
    `agentflow_memory::TokenCounter` trait + matching
    `HeuristicCounter` default + four `*_with_counter`
    constructors (`new_with_counter`, `user_with_counter`,
    `assistant_with_counter`, `system_with_counter`,
    `tool_result_with_counter`) preserve every existing
    `Message::new` callsite as the heuristic path — tests
    that don't care about precision keep working — and add a
    parallel precise path for callers that do.
  - `agentflow-agents::token_counter_adapter::LlmTokenCounter`
    bridges the gap between `agentflow_llm::TokenCounter`
    (BPE-backed, lives in agentflow-llm) and
    `agentflow_memory::TokenCounter` (the local trait the
    `Message::*_with_counter` signature requires).
    `build_message_counter(model_id) -> Box<dyn
    agentflow_memory::TokenCounter>` is the convenience
    constructor used by the agent runtimes.
  - `ReActAgent` and `PlanExecuteAgent` gained
    `message_counter: Box<dyn TokenCounter>` fields,
    initialised from `config.model` in `new()` and rebuilt in
    `apply_context()` when the run-time context overrides the
    model. Every production `Message::user / assistant /
    system / tool_result` call inside the two agents (15 sites
    total) now routes through `*_with_counter(&self.session_id,
    content, &*self.message_counter)` so the per-message
    `token_count` matches what the LLM provider actually bills.
  - Direct consequence: `apply_memory_prompt_budget` in
    `ReActAgent` now compacts the history against precise BPE
    counts for the OpenAI family (gpt-3.5/4/4o/o1/o3) and the
    OpenAI-compat vendors that share cl100k_base (Moonshot,
    DeepSeek, GLM, DashScope, MiniMax, StepFun). CJK text and
    code that the heuristic over-estimates by 3-5× no longer
    triggers premature compaction; English text that the
    heuristic under-counted no longer ships over-budget
    prompts that providers reject.
  - 9 new hermetic tests:
    - 6 in `agentflow-memory::types::tests` (heuristic preserved
      for `Message::new`, counter respected for
      `new_with_counter`, role + tool_name preserved through
      the `*_with_counter` variants, `.max(1)` token floor
      invariant for empty content, heuristic-vs-counter
      divergence proven on CJK input).
    - 3 in `agentflow-agents::token_counter_adapter::tests`
      (tiktoken routing for OpenAI family, heuristic fallback
      for non-BPE families, trait routing through the
      `Box<dyn TokenCounter>` boundary).
  - Test-site decision: the ~50 callsites in `agentflow-memory
    /tests/*` and `agentflow-agents/tests/prompt_assembly*`
    stay on the heuristic path. They're testing message-
    handling logic (search, compaction, eviction order), not
    tokenization accuracy. Forcing them onto the BPE counter
    would change the expected eviction boundaries and require
    rewriting every fixture's expected token counts. The
    precision improvement that matters lands at the agent
    production layer; that's where the LLM provider actually
    sees the prompt.
  - Verification: cargo build --workspace --tests + cargo
    clippy --workspace --tests -D warnings + cargo test -p
    agentflow-agents -p agentflow-memory (memory lib 42, +6
    new; agents total 194, +3 new from adapter tests).

- DONE P10.3.4 / P10.18.1 (Low — v1.x) `cargo xtask refresh-live-models`
  - Landed as a new `refresh-live-models` xtask subcommand that
    pings each of the 9 live-test providers' `/models` endpoints,
    verifies the hard-coded text-model default in
    `agentflow-llm/tests/provider_consistency_live.rs::run_text_path`
    is still listed, and prints a per-provider verdict +
    suggestions when a default is missing. Closes both the P10.3.4
    auto-rotate entry and the P10.18.1 stretch (they were
    cross-referenced TODOs for the same task).
  - **Scope decision**: validate-only, no auto-edit. The xtask
    reports `ok` / `missing` / `skipped` / `error` and suggests up
    to 3 replacement ids per missing default, ranked by shared
    prefix with the current default. The operator copies the
    suggestion into the test source manually. Rationale: the test
    file's defaults carry inline rationale comments (e.g.
    `claude-3-5-haiku-latest` alias and the dated `-20241022`
    revision both 404 against current Anthropic tiers — see
    CLAUDE.md) that an automated rewrite would clobber. A future
    `--apply` mode is straightforward to add when demand for full
    automation surfaces.
  - **Providers covered (all 9)**: openai / anthropic / google /
    moonshot / stepfun / glm (Zhipu/BigModel) / dashscope /
    deepseek / minimax. Each probe uses the same auth shape
    `agentflow-llm`'s providers use:
    - OpenAI-compat (`Authorization: Bearer`) — openai + 6 vendors
    - Anthropic (`x-api-key` + `anthropic-version: 2023-06-01`)
    - Google (`?key=<KEY>` URL param, plus `models/` prefix
      stripping on the response — without that, every Google
      default would always report missing)
  - **Key resolution**: loads `~/.agentflow/.env` into the process
    environment (silently no-op when missing, the expected CI
    case where keys come from the workflow's `env:` block). The
    loader explicitly treats an existing empty env var as
    unset — an exported-but-empty `OPENAI_API_KEY=` would
    otherwise silently block a valid `.env` value. Per-provider
    multi-key fallback matches `agentflow-llm`'s resolution order
    (e.g. GLM tries `GLM_API_KEY` then `BIGMODEL_API_KEY` then
    `ZHIPU_API_KEY`).
  - **Implementation note**: shells out to `curl` rather than
    pulling `reqwest` into xtask's dep graph. Trade-off: one
    process spawn per probe + uses serde_json for response parse.
    Keeps xtask compile-time short and dep graph minimal — this
    is an operator tool, not a hot path.
  - **Flags**: `--keep-going` (don't exit non-zero on per-provider
    HTTP errors — useful when one provider's auth is broken but
    you want the other 8 reports), `--include <name>` (repeatable,
    restrict to a subset; unknown names fail fast with the full
    provider list).
  - **First real-world run surfaced two findings** against
    `~/.agentflow/.env`:
    - `glm/glm-4.5-flash` is no longer in `/v1/models`; suggested
      replacements `glm-4.5-air`, `glm-4.5`, `glm-4.6`. (Real
      deprecation; operator action needed.)
    - `anthropic/claude-haiku-4-5` shows as missing because
      `/v1/models` lists only dated revisions
      (`claude-haiku-4-5-20251001`, etc.), not rolling aliases.
      The rolling alias still resolves in actual API calls — this
      is an Anthropic-specific false positive of the
      "is the id in the list" check. Documented in the tool's
      help / the operator can choose to pin a dated revision or
      stay on the alias.
  - **Tests (9 new in `xtask::refresh_live_models_tests`, all
    hermetic — no network)**: 5 response-parser tests (OpenAI-
    compat / Anthropic / Google + models/ prefix stripping /
    missing-id-field skip / shape-error message), 1 shared-prefix
    length sanity, 2 dotenv quote-stripping cases, 1 unknown-
    include filter rejection, 1 probe_skips_providers_without_
    api_key. The shape-error test pins the "missing `data` array"
    diagnostic so a stale-response regression surfaces cleanly.
  - **Verification**: `cargo build -p xtask` clean. `cargo test
    -p xtask refresh_live_models` 9/9 green. `cargo clippy -p
    xtask --tests -- -D warnings` clean. End-to-end smoke
    against `~/.agentflow/.env` printed the expected 6 ok / 2
    missing / 1 error breakdown.

### P10.4 — agentflow-tools (A-)

No gaps from the evaluation. Future opportunities:

- DONE P10.4.1 (Stretch) Sandbox profile per-tool override
  - Landed via `[[tools]] os_sandbox = true|false` on individual
    tool blocks (rather than the TODO's suggested
    `[security.tools.<name>] os_sandbox = "enforcing"` shape).
    Rationale: a boolean per `[[tools]]` block stays consistent
    with the existing manifest-level `[security] os_sandbox`
    flag, and the runtime `SandboxEnforcement::{Enforcing,
    Permissive, Disabled}` tri-state is platform-determined
    output (depends on whether `sandbox-exec` / `seccomp` is
    available), not user-configurable from the manifest. The
    operator-facing question is purely "request enforcement on
    this tool or not" — `true` / `false` captures it without
    introducing a stringly-typed config the schema layer would
    have to validate.
  - Schema: new `os_sandbox: Option<bool>` field on
    `agentflow_skills::manifest::ToolConfig`. `None` (the
    default, and what every pre-P10.4.1 manifest deserialises to)
    falls back to `security.os_sandbox`; `Some(true)` opts that
    tool in even when the manifest-level default is off;
    `Some(false)` opts it out even when the manifest-level
    default is on. Resolved per-iteration in
    `agentflow_skills::builder::build_tool_registry` so a mixed-
    policy skill (sandbox shell but not script, or vice versa)
    gets exactly what the manifest declares.
  - Scope: only `shell` and `script` honour the override —
    they're the tools that actually spawn subprocesses. `file`
    and `http` ignore the field (documented in the rustdoc; the
    builder simply doesn't read it for those tool kinds).
  - Operator UX: `agentflow skill inspect --explain-permissions`
    now prints a per-tool resolution table under
    `Sandbox profile`. Each sandboxable tool gets a line like
    `  shell: true (per-tool override)` or `  script: false
    (inherited)` so the resolved value is visible at a glance —
    the manifest-level line alone hid the effective state once
    overrides existed.
  - Backwards compatibility: zero — the field is optional with
    `None` default, so manifests authored before this change
    parse identically and the builder's behaviour for them is
    unchanged. The new field is additive on the wire.
  - Tests (6 new in `builder::tests`, plus 1 serde
    round-trip): per-tool=true overrides manifest=false (opt
    in), per-tool=false overrides manifest=true (opt out),
    fallback inherits manifest=on, fallback inherits
    manifest=off, the canonical "heterogeneous skill"
    (`shell` sandboxed + `script` unsandboxed in the same
    manifest), and a serde stability pin (`ToolConfig` accepts
    `os_sandbox = true|false` on a `[[tools]]` block; the field
    is fully optional so pre-FU manifests still parse).
    Backend-name assertions use `Tool::sandbox_status().backend`
    + `agentflow_tools::sandbox::default_backend()` so the
    suite stays portable across hosts that fall back to noop.
  - Verification: `cargo test -p agentflow-skills --lib
    builder::tests` 23/23 green (6 new). `cargo clippy -p
    agentflow-skills -p agentflow-cli --tests -- -D warnings`
    clean.
  - Docs: `docs/TOOL_PERMISSIONS.md` gains a "Per-tool override
    (P10.4.1)" subsection with the TOML example + resolution
    rules + the `skill inspect` UX note.

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

- DONE P10.6.2 (Medium) Additional eval baselines
  - Generated against `OPENAI_API_KEY` from `~/.agentflow/.env`:
    `dense.json` (retriever=`dense`, model=`text-embedding-3-small`,
    Recall@1=0.65, Recall@5=1.0, MRR=1.0) and `hybrid.json`
    (`hybrid:bm25+dense:text-embedding-3-small`, RRF k=60, same
    metrics on the `ci_offline` dataset). Both checked in under
    `agentflow-rag/eval_baselines/ci_offline/`.
  - Round-trip verified by re-running `--compare-baseline` against
    each file: regression gate `PASS — no regression: neither
    recall nor p-value crossed the threshold` on both. Determinism
    holds at the threshold level (Recall@5 / nDCG@5 deltas =
    0.0000 on rerun) even though OpenAI embedding numerics aren't
    bit-stable.
  - Discovered + fixed a pre-existing reader / writer mismatch:
    `--output <path>` writes the `{ dataset, baseline, candidate,
    comparison, regression }` envelope but `--compare-baseline`
    previously only parsed a bare `EvalReport`. The dual-shape
    reader in `load_baseline_from_path` now accepts both shapes
    (bare EvalReport first; falls through to `envelope["baseline"]`
    extraction on parse failure) so operators can feed their own
    `--output` files back without manual extraction. 3 new unit
    tests in `commands::rag::eval::tests` pin the dual-shape
    contract + the actionable error on neither-shape-matches.
  - CI: `.github/workflows/quality.yml::rag-eval-smoke` extended
    with two new steps that gate against `dense.json` + `hybrid.json`
    when `OPENAI_API_KEY` secret is set on the runner. `if: ${{
    secrets.OPENAI_API_KEY != '' }}` (NOT `env.OPENAI_API_KEY` —
    the `if:` evaluates before the step `env:` block) lets forks
    without the secret stay green.
  - Docs: `docs/RAG_EVAL.md` gains a "Checked-in regression
    baselines" subsection with the three-baseline table + the
    regeneration commands.

- DONE P10.6.3 (Low — Stretch) Latency profile per chunk size
  - Landed as a new `--chunk-size <N>` flag on `agentflow rag eval`
    plus the supporting library plumbing in `agentflow-rag::eval`.
    When set, the eval re-chunks every corpus doc with a
    `FixedSizeChunker(N, overlap=0)` before building the retriever
    index. Retrieved chunk ids are remapped back to source doc ids
    (with dedupe within the top-K window) before qrels scoring, so
    `Recall@K` / `MRR` / `nDCG@K` stay comparable across chunked
    and un-chunked runs — qrels still reference source doc ids,
    not synthetic chunk ids. The latency block, however, naturally
    reflects the chunked index shape (N× more documents indexed at
    smaller sizes), which is the operator-facing signal the TODO
    asks for.
  - **Scope decision**: single-value `--chunk-size N` (not a sweep)
    per CLI invocation. The operator captures one baseline per
    chunking strategy and diffs the JSON files manually, the same
    workflow the existing `--retriever {bm25,dense,hybrid}` axis
    uses today. Sweeping multiple sizes in one invocation would
    multiply the comparison-block / regression-gate semantics
    confusingly; deferred unless concrete operator demand surfaces.
  - **New library surface in `agentflow-rag::eval`**:
    - `chunking_eval` module: `ChunkedDataset` (chunked corpus +
      synthetic-id-to-source-id map + chunker config),
      `chunk_dataset(&Dataset, chunk_size, overlap) -> Result<…>`,
      `remap_chunks_to_doc_ids(&[String], &HashMap<String, String>)
      -> Vec<String>` (dedupes within the top-K window).
    - `evaluate_with_remapping(retriever, dataset, &Option<map>,
      config)`: the existing `evaluate()` is now a thin wrapper
      passing `None`. The runner over-fetches `K × 8` candidates
      from the retriever in chunked mode so the dedupe step
      still yields K distinct source docs.
    - `EvalReport.chunk_size: Option<usize>` (additive,
      `#[serde(default, skip_serializing_if = "Option::is_none")]`).
      Pre-P10.6.3 baselines without the key deserialise as `None`
      so the three checked-in baselines under
      `agentflow-rag/eval_baselines/ci_offline/*.json` continue to
      parse cleanly.
  - **CLI surface**: `--chunk-size <N>` honored by every retriever
    backend (bm25 / dense / hybrid). The dense path re-embeds the
    chunked corpus, which means dense + a small chunk_size N
    multiplies the OpenAI embedding-API cost by `corpus_size_after_
    chunking / corpus_size`. Documented in `docs/RAG_EVAL.md` so
    operators don't run a 1024-doc corpus through dense at
    chunk_size=64 by accident.
  - **Compat warning**: when `--compare-baseline` is supplied and
    the stored baseline's `chunk_size` differs from the current
    run's, the CLI prints a stderr warning ("baseline chunk_size=
    Some(N) differs from current chunk_size=Some(M); metric
    deltas may reflect chunking-strategy change, not pure
    retriever drift") but does NOT fail — cross-chunk comparison
    is still useful for "did the chunking change hurt recall?"
    investigations.
  - **Renderer**: when `chunk_size` is `Some`, the text-mode report
    table adds a `Chunk size: N (fixed-size, overlap=0)` line
    directly under the latency block. When `None`, the table is
    byte-identical to pre-P10.6.3.
  - **Tests (12 new across library + CLI, all hermetic — BM25
    only, no OpenAI)**:
    - 9 in `eval::chunking_eval::tests`: chunk_size=0 rejection,
      overlap>=chunk_size rejection, synthetic-id + mapping
      invariants, query/judgment pass-through, title+body
      concatenation matches `Bm25Eval::from_dataset` convention,
      empty-doc → single empty chunk fallback, remap-dedupe
      preserves rank order, unknown chunk-id passes through
      unchanged, full chunked-BM25-eval round trip (proves
      remap+dedupe restores recall vs the un-remapped chunk-id-
      only result).
    - 2 in `eval::runner::tests`: `render_table` omits the chunk-
      size line when `None` (byte-identical to legacy), surfaces
      it when `Some`. Plus a serde round-trip test proving the
      field is optional + back-compat on read.
    - 2 in `agentflow-cli/tests/rag_eval_cli_tests.rs`:
      `--chunk-size 64` produces a report with
      `baseline.chunk_size = 64` and the "Chunk size:" line on
      stdout; the un-chunked run has neither (key absent, not
      null).
  - **Verification**: `cargo test -p agentflow-rag --lib eval::`
    82/82 green; `cargo test -p agentflow-cli --features rag
    --test rag_eval_cli_tests` 7/7 (2 new); `cargo clippy
    -p agentflow-rag -p agentflow-cli --features rag --tests
    -- -D warnings` clean.
  - **Docs**: `docs/RAG_EVAL.md` gains a "Per-chunk-size latency
    profile (P10.6.3)" subsection with the three-baselines
    capture recipe + the text-output sample + the
    `--compare-baseline` warning behavior.

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

- DONE P10.7.2 (Low — v1.x) Encryption-at-rest implementation (age)
  - Landed `AgeEncryptedPreferenceStore<S: PreferenceStore =
    SqlitePreferenceStore>` in
    `agentflow-memory/src/preference_encrypted.rs`. Wraps any
    `PreferenceStore` impl, transparently age-encrypts every
    `value` payload on write + decrypts on read. Keys (tenant /
    user / key string) stay plaintext on disk — the inner store
    needs them for queries — only the *value* is opaque to anyone
    without the identity file.
  - **KMS decision**: `age` X25519 single-user identity. No cloud
    dependency, no envelope re-keying, no per-record key wrapping.
    Sufficient for the local profile per `docs/MEMORY_LAYERING.md`
    §3 "Encrypted at rest is optional; the trait should support
    it but a plaintext default is acceptable for the local
    profile." Cloud KMS / multi-user / envelope re-keying are
    deferred to a v2 design conversation per
    `docs/ROADMAP_v2.md` Theme B.
  - **On-disk shape**: encrypted values live in the inner store's
    existing `value` column as a JSON string with the form
    `"age:v1:<base64(age-ciphertext)>"`. The `age:v1:` marker
    prefix lets a reader verify they're looking at ciphertext
    rather than plaintext — get-on-a-plaintext-row now hard
    fails with a clear "missing marker" error so a stale
    `SqlitePreferenceStore` write can't silently bleed back
    through the encrypted wrapper. Future migrations bump the
    `v1` suffix without breaking back-compat.
  - **Identity-file helpers**:
    - `generate_identity_file(path)` — generates a fresh X25519
      identity, writes the canonical `AGE-SECRET-KEY-1...`
      representation to `path` with mode 0600 on Unix. Refuses
      to overwrite an existing file (`refusing to overwrite
      existing identity file at …`) so a misconfigured operator
      can't accidentally clobber a key + lose every encrypted
      preference.
    - `load_identity_file(path)` — reads + parses the same
      format. Trims trailing whitespace.
    - Convention: `~/.agentflow/identity.age` (operator-chosen;
      not hard-coded so multi-tenant deploys can host per-tenant
      identity files).
  - **`open_sqlite(db_path, identity_path)`** convenience
    constructor for the common case. Production callers pair an
    identity file path with the existing
    `SqlitePreferenceStore::open(...)` data path.
  - **Threat model documented inline**: protects against an
    attacker with read access to the SQLite file but not the
    identity. Does NOT protect against process-memory inspection
    or timing/size side-channels. Operator pairs with host-level
    disk encryption (FileVault, LUKS, etc.) for defense in depth.
  - **Tests (12 new in `preference_encrypted::tests`, all
    hermetic — in-memory SQLite, freshly-generated identities,
    no fs except tempfile for identity-file tests)**:
    - 4 trait-pass-through tests: round-trip simple value,
      round-trip complex JSON, version-increment on second
      `put`, delete idempotency, `list_decrypts_every_row`,
      `prune_passthrough_uses_inner_store`.
    - 3 security tests: `ciphertext_is_not_recognizable_as_the_
      plaintext` (peeks at the inner store's stored value to
      confirm the plaintext substring is absent + the marker
      prefix present), `decrypt_with_wrong_identity_fails`
      (two distinct identities → ciphertext encrypted to A is
      unreadable by B), `get_rejects_plaintext_row_missing_
      marker` (plaintext bleed-through is hard-failed).
    - 3 identity-file tests: round-trip generate→load,
      refuses-to-overwrite-existing-file, mode-0600 on Unix.
  - **Wiring**: `lib.rs` re-exports `AgeEncryptedPreferenceStore`
    + `generate_identity_file` + `load_identity_file` at crate
    root. CLI integration (a new `--encrypted --identity <path>`
    flag pair on `agentflow memory prune` etc.) is a separate
    follow-up — the trait surface is identical, so any future
    CLI command can `Box<dyn PreferenceStore>` over either
    backend at runtime.
  - **Verification**: `cargo build -p agentflow-memory` clean
    after `age = "0.11"` + `base64 = "0.22"` deps added.
    `cargo test -p agentflow-memory preference_encrypted` 12/12
    green. `cargo clippy -p agentflow-memory --tests -- -D
    warnings` clean.

- DONE P10.7.3 (Low — v1.x) Cross-session memory linking strategy
  - Resolved as a v2 design RFC at
    `docs/RFC_CROSS_SESSION_MEMORY_LINKING.md`, not as code. The
    TODO was explicit ("v2 design conversation"); closing it means
    framing the conversation, not pre-deciding the implementation.
    Same shape as the P10.10.1 (H6) and P10.19.1 (WASM) 1-pagers
    — decide-when-to-revisit, persist the analysis, let demand
    drive the next P11.x rather than speculation.
  - **Honest framing**: three of the four memory layers already
    cross sessions automatically (preferences are user-scoped not
    session-scoped; entity facts are entity-scoped; semantic
    memory does cross-session vector search). What's missing is
    a *navigable index* — given an entity, find its session
    footprint; given a session, find its entity neighbours.
  - **5 use cases documented** (session inventory per entity,
    session resumption with entity context, implicit knowledge-
    graph queries, cross-session fact conflict surfacing, time-
    bounded session linkage). Each checked against "can the
    current four-layer model do this?" so the implementation
    discussion stays grounded.
  - **3 design options** in order of ambition: (A) index-only
    additive SQLite tables (covers 3/5 UCs at lowest cost),
    (B) embedded graph store (oxigraph / indradb / cozo —
    covers all 5 natively), (C) vector-graph hybrid (builds on
    existing semantic layer, statistical not symbolic).
  - **No recommendation** pre-committed — the RFC names
    Option A as the pragmatic default *when* promotion fires,
    but explicitly parks B + C as exploratory tracks until
    concrete UC3-shaped demand surfaces.
  - **4 promotion criteria** pinned (concrete operator request,
    3+ skill manifests hand-rolling session-inventory queries,
    4+ week Harness session conflict-resolution incident,
    external/forked-deployment demand). Until one fires, the
    four-layer baseline covers durable memory + the maintenance
    + complexity cost of linkage doesn't pencil out.
  - **5 open questions** documented (write-path overhead,
    conflict detection cadence, entity ID stability,
    long-running-session semantics, retention-cascade
    interaction) so a future P11.x promotion has the
    homework already done.
  - `docs/ROADMAP_v2.md` Theme B updated to point at the RFC
    + mark P10.7.3 closed.

### P10.8 — agentflow-agents (A-)

No active gaps. Future opportunities:

- DONE P10.8.1 (Stretch) ReAct trace replay diff tool
  - Landed as a new `agentflow agent` namespace + `replay` subcommand:
    `agentflow agent replay <current.jsonl> --diff <baseline.jsonl>
    [--strict-tokens] [--format text|stream-json|json-envelope]`.
    Pure file-to-file comparator — no LLM call required at runtime.
    The user produces both JSONL files however they like (running
    ReAct twice + capturing AgentEvent streams, extracting from the
    server's persisted events, etc.).
  - **Scope decision**: this is NOT a "run-then-diff" wrapper. The
    TODO's framing ("compare a fresh ReAct run against a golden
    trace") was relaxed to file-to-file because actually running
    ReAct fresh requires real LLM API access (real-environment
    territory). The comparator + the JSONL contract is the
    valuable piece; a future `agentflow agent run + --diff`
    wrapper can land as a separate subcommand once `agent run`
    exists (it doesn't today).
  - **Three diff dimensions** (matching the TODO's "tool-call
    order, message tokens, stop-reason"):
    1. Step order: walks the common prefix of the two
       `step_completed` event streams, flags `StepKindMismatch`
       (different `AgentStepKind` discriminants) and
       `ToolNameMismatch` (same kind=tool_call but different
       tool). Tool *params* differences alone are NOT flagged —
       LLM-driven params jitter is expected noise.
    2. Stop reason: terminal `RunStopped.reason` variants
       compared via their snake_case label
       (`final_answer` / `max_steps` / etc.). Missing terminal
       event on one side counts as a divergence too.
    3. Token usage: per-step `LlmCallCompleted.prompt_tokens` /
       `completion_tokens` deltas reported as soft `Variance`
       lines by default (since LLM token accounting jitters by a
       few tokens between identical requests).
       `--strict-tokens` promotes any non-zero delta to a hard
       `Divergence` that fails the gate.
  - **CLI surface**: `text` mode prints a human-readable summary;
    `stream-json` emits one `{type, data}` line per
    divergence / variance to stdout for piping into automation;
    `json-envelope` wraps the full structured `DiffReport` in
    the canonical `agentflow.cli/1` envelope so CI consumers can
    parse the result.
  - **Exit code**: zero on perfect match (variances allowed
    unless `--strict-tokens`); non-zero on any `Divergence`.
  - **`AgentEvent` namespace boundary**: this is operationally
    different from `agentflow harness replay`. Harness Mode emits
    `HarnessEvent` (workspace-aware, approval-gated, seq-keyed);
    ReAct emits `AgentEvent` (fine-grained: per-step, per-LLM-
    call, per-tool-call). The two enums have different wire
    shapes, and the diff tool only operates on the latter. The
    namespace separation (`agent` vs `harness` top-level
    subcommands) makes the boundary discoverable.
  - **Tests (20 new across 2 layers, all hermetic)**: 14 pure
    comparator + parser tests in
    `commands::agent::replay::tests` (identical-traces /
    step-count rollup / step-kind mismatch / tool-name mismatch
    / params-alone-is-not-divergence / stop-reason mismatch
    (both-sides + one-side-missing) / token-delta-as-variance /
    token-delta-as-divergence-under-strict / missing-llm-call
    delta-against-zero / JSONL parse: blank-line tolerance +
    real event shape round-trip + line-number-on-malformed +
    last-run_stopped-wins + format parser). 6 hermetic CLI
    integration tests in `agentflow-cli/tests/agent_replay_tests.rs`
    drive the CLI binary end-to-end: identical-traces-success,
    tool-name-divergence-fails-non-zero, stop-reason-divergence-
    fails-non-zero, json-envelope-shape, --strict-tokens-promotes-
    delta-to-divergence, malformed-JSONL-reports-line-number.
  - **Verification**: `cargo build -p agentflow-cli` clean;
    `cargo test -p agentflow-cli --lib commands::agent`
    14/14; `cargo test -p agentflow-cli --test agent_replay_tests`
    6/6; `cargo clippy -p agentflow-cli --tests -- -D warnings`
    clean.

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

- DONE P10.9.2 (Low — Stretch) Skill marketplace search UX
  - Landed the CLI JSON-envelope half of the TODO. `agentflow
    marketplace search` now accepts `--format text|json|json-envelope`
    (default `text`, preserving the existing human-readable
    output). `--format json` emits the structured payload; `--format
    json-envelope` wraps it in the canonical `agentflow.cli/1`
    envelope shape so script consumers can parse without scraping
    stdout.
  - **Scope decision**: the TODO also mentioned a Web UI
    marketplace browser tab. Deferred — `P10.17.1` committed the
    UI surface to debugger-only positioning (run console, harness
    session view, trace replay), and the marketplace browser is
    explicitly out of that scope. If concrete operator demand
    surfaces, it'd open as a new `P11.x` item per the
    `docs/ROADMAP_v2.md` promotion workflow.
  - **Payload shape**:
    ```json
    {
      "registry": "<URL or path>",
      "query": "<query string or null>",
      "package_type_filter": "skill" | "plugin" | null,
      "manifest": { "schema_version", "name", "description",
                    "homepage", "total_entries" },
      "entries": [ RemoteMarketplaceEntry, ... ],
      "matched_count": <usize>
    }
    ```
    Empty matches produce `entries: []` + `matched_count: 0`
    (never null or missing) so scripts can iterate without
    special-casing the no-result path.
  - **Envelope shape**: `{version, command, result, errors}` only,
    with the `result` body byte-identical to the bare `--format
    json` output (the additive-field contract the other
    `--format json-envelope` migrations pin).
  - **Reuse**: shared `build_search_payload(...)` between the two
    JSON paths so they can't drift; the only difference is the
    envelope wrapper.
  - **Tests (4 new in `agentflow-cli/tests/marketplace_cli_tests.rs`,
    all hermetic — local TOML files, no HTTP)**:
    - `marketplace_search_json_format_emits_structured_payload`
      (pins every top-level key + the entry list shape +
      filter-survival into `package_type_filter`).
    - `marketplace_search_json_envelope_wraps_body_in_canonical_shape`
      (captures `--format json` body first, then envelope, and
      asserts `result == legacy_body` + version + command +
      top-level keys exactly `[command, errors, result, version]`).
    - `marketplace_search_json_format_empty_match_set_renders_empty_entries`
      (pins the never-null contract).
    - `marketplace_search_unknown_format_is_rejected_by_clap`
      (`value_parser` enforcement so a fat-fingered CI flag
      doesn't fall through to text mode).
  - **Verification**: `cargo build -p agentflow-cli` clean.
    `cargo test -p agentflow-cli --test marketplace_cli_tests
    marketplace_search` 5/5 green (1 pre-existing + 4 new).
    `cargo clippy -p agentflow-cli --tests -- -D warnings` clean.

- TODO P10.9.2-FU1 (Low — Stretch) Web UI marketplace browser tab
  - Carry-forward of the second half of the original P10.9.2 TODO.
    P10.17.1 committed the Web UI to debugger-only positioning,
    so this stays Stretch until a concrete operator complaint
    surfaces; promote to a real P11.x line once that happens, per
    `docs/ROADMAP_v2.md`.

### P10.10 — agentflow-harness (A-)

- DONE P10.10.1 (Medium — v1.x) Promote individual H6 items from
  `Later Tracks` on concrete demand
  - Resolved as a 1-pager (`docs/H6_PROMOTION_CRITERIA.md`)
    capturing per-item promotion triggers, not as code. The
    TODO was a *tracking* item — "don't pull en bloc, write
    an RFC per item when demand appears" — so closing it
    means pinning the gate, not shipping any of the five
    items. Same shape as P10.19.1 (WASM 1-pager): decide-when-
    to-revisit, persist the analysis, let demand drive the
    next P11.x rather than speculation.
  - For each of the 5 H6 items (slash-command expansion, TUI
    product shell, OpenHarness-style config import, plugin
    compatibility adapters, provider subscription bridge) the
    doc pins: what concrete demand signal tips the scale, the
    RFC scope when it does, the estimated effort, and an
    explicit cross-reference to the non-goal stance in
    `RoadMap.md` for the two items that are currently
    documented as non-goals (TUI product shell, provider
    subscription bridge).
  - `docs/HARNESS_MODE.md` H6 row links to the criteria doc;
    `docs/ROADMAP_v2.md` Theme F is updated to note the
    triggers + non-goal flags. Future P11.x promotion opens
    a per-item RFC under `docs/RFC_H6_<slug>.md` referencing
    the criteria doc.

- DONE P10.10.2 (Low — Stretch) Harness session replay
  - Landed: new `HarnessCommands::Replay` subcommand backed by
    `agentflow-cli/src/commands/harness/replay.rs::execute`. The
    "JSONL→TUI renderer" in the TODO scope reduced to "per-event
    formatted lines on stdout" — a real TUI is overkill for the
    stretch tier, and stream-json mode covers the
    automation-friendly path. The pacing logic (`SpeedMode::
    Realtime(multiplier)` / `Instant`) is what makes this
    materially different from the existing `resume` (dump-all-at-
    once) command.
  - Flags: `--speed` (`1x` / `2x` / `0.5x` / `inf` / `instant`,
    case-insensitive on the aliases; bare integers rejected with
    a clear "must end in 'x'" message), `--from-seq` / `--to-seq`
    (inclusive bounds, u64-typed to match `HarnessEvent.seq`),
    `--filter-kind` (repeatable, OR semantics over the snake_case
    kind discriminator), `--output {text, stream-json}` (json /
    json-envelope rejected up front as bounded formats).
  - Robustness: 1-hour sleep cap so a session that idled overnight
    doesn't hang the replay; `Duration::ZERO` for backwards-ts
    (clock skew) so out-of-order events flow through instead of
    panicking; `infx` rejected as non-finite (the parse-as-f64
    edge case) so it doesn't silently degrade to Instant.
  - Tests: 15 unit in `commands::harness::replay::tests`
    (parse_speed: every accepted form + each rejection path;
    apply_filters: no-filter / from / to / kind / additive
    include-list; sleep_between: Instant / multiplier scaling /
    backwards-ts / 1-hour cap) + 7 hermetic CLI tests in
    `agentflow-cli/tests/harness_replay_tests.rs` that seed a
    temp JSONL session log with `chrono::TimeZone::timestamp_opt`
    + run the CLI binary against it (instant-speed full stream,
    stream-json one-event-per-line + header-on-stderr,
    filter-kind restriction, from-seq skip, bare-integer-speed
    rejection, json-envelope-format rejection, unknown-session
    error). `cargo clippy -p agentflow-cli --tests -- -D
    warnings` clean.

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

- DONE P10.11.3 (Low — Stretch) Remaining `--format json-envelope`
  migrations
  - Audit via `awk` over `agentflow-cli/src/main.rs::value_parser`
    fields: the audit found EXACTLY ONE holdout — `mcp config
    list`'s `--format` accepted only `text | json`. Every other
    `format: String` clap field already accepted `json-envelope`
    (or `stream-json` where streaming is the contract).
    `mcp config show` was an intentional exception (bare JSON
    always, no `--format` at all) — its output is a single
    bounded server config, and adding `--format` would be a
    breaking change for callers that pipe to `jq` directly.
  - Migrated `mcp config list`:
    `value_parser` widened to `["text", "json", "json-envelope"]`;
    `run_list(format: &str)` instead of `run_list(json: bool)`;
    json-envelope mode wraps the same `{source, servers}` body
    in `CliJsonEnvelope::ok("mcp config list", ...)`. Legacy
    `--format json` bare-body output preserved unchanged so
    existing scripts don't break. New hermetic CLI test
    (`config_list_json_envelope_format_wraps_body_in_canonical_envelope`)
    sits alongside the existing `_json_format_emits_structured_payload`
    test and pins the four canonical envelope fields
    (`version`, `command`, `result`, `errors`) + asserts the
    body parity with the legacy mode.

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

### P10.13 — agentflow-viz (closed: crate deleted 2026-05-20)

- DONE P10.13.1 (Medium — v1.x) Decide: merge with `agentflow-ui`
  OR establish live-trace interop protocol → **Decision: delete
  the crate entirely.** An honest audit revealed the UI's
  "DAG visualisation" surface was a button grid of node status
  badges plus the raw Mermaid markdown text in a `<pre>` block —
  no SVG, no spatial layout, no edges. The data-plumbing cost
  (an entire workspace crate + a beta REST route + a CLI
  subcommand + the UI fetch path) was disproportionate to the
  rendering value. Removed: `agentflow-viz/` crate,
  `/v1/runs/{id}/graph` REST endpoint, `agentflow workflow
  graph` CLI subcommand, `RunGraphResponse` shape, the Web UI
  Mermaid `<pre>` block + `runGraph` state. The node-status
  grid in the UI is now derived entirely from event payloads
  (already the source of truth for execution state). Updated
  references: `docs/STABILITY.md` (Beta row removed),
  `docs/WEB_UI.md` (DAG dependency + viz reference),
  `docs/ROADMAP_v2.md` Theme D (decision rationale),
  `docs/ARCHITECTURE.md`, `AGENTS.md`, `OVERALL_EVALUATION_REPORT.md`,
  `docs/CI_WORKFLOWS.md`, `docs/RELEASE_CHECKLIST.md`,
  `docs/CURRENT_STATUS.md`, `docs/AGENT_EVAL_FORMAT.md`,
  `docs/DEPLOYMENT.md`, `CLAUDE.md`. Future RFC may revisit
  graphical DAG / agent topology rendering as an additive
  UI-only feature.

### P10.14 — agentflow-server (A-)

No active gaps beyond the v1.0.0-rc.1 ops (P10.0). Future:

- DONE P10.14.1 (Medium — v1.x) Per-run retention override via
  POST body
  - Landed. `POST /v1/runs` body now accepts
    `retention_overrides: {events_days, artifacts_days}` (both
    optional). The cleanup sweep uses `max(global, override)`
    semantics so an override can only ever extend retention.
    Pinning events or artifacts also pins the `runs` row itself
    (via `GREATEST(global, events_override, artifacts_override)`
    on the run-row delete) so the `ON DELETE CASCADE` doesn't
    yank the pinned children out from under the override.
    Negative overrides → `bad_request`; `Some(0)` → normalized
    to NULL.
  - New migration `0005_run_retention_overrides.sql` adds two
    nullable INTEGER columns to `runs`; safe additive upgrade
    (existing rows default to NULL ≡ no override).
  - New types: `RetentionOverrides` in `agentflow-server::runs`
    (re-exported from `agentflow-server::lib`), with
    `validate()` + `into_pair()` API. 7 unit tests cover
    validation, normalization, and partial-body deserialization.
  - New integration tests in
    `agentflow-server/tests/cleanup_route.rs`:
    `cleanup_skips_terminal_run_pinned_by_events_override` (run
    row pin) and `cleanup_skips_events_pinned_by_override`
    (events-sweep pin). Both self-skip without
    `AGENTFLOW_DATABASE_TEST_URL` to keep workspace `cargo test`
    hermetic.
  - New route-level tests in `runs_routes.rs`:
    `submit_run_persists_retention_overrides`,
    `submit_run_rejects_negative_retention_override`,
    `submit_run_normalizes_zero_override_to_null`.
  - Docs: `docs/DEPLOYMENT.md` "Per-run retention overrides"
    snippet for the operator-facing curl example.

- DONE P10.14.2 (Low — v1.x) Operational dashboards (Grafana
  templates)
  - Landed `dashboards/grafana/agentflow-overview.json` + the
    `dashboards/README.md` operator playbook. The dashboard
    carries 9 panels (system health, active runs per tenant,
    workflow completions by status, p50/p95/p99 duration, node
    failures by node_type, worker fleet, memory + state size,
    retention sweep deletions, Harness Mode sessions +
    approvals) against the metric-name contract documented in
    `docs/KUBERNETES_DEPLOYMENT.md` §Grafana Dashboard.
  - **Honest gap:** the TODO note assumed "server emits
    Prometheus metrics," but the in-core metrics module was
    removed during the observability split and
    `agentflow-server` doesn't expose `/metrics` today. The
    dashboard is forward-compatible — it will render the
    moment emission lands. The dashboard JSON shipping *now*
    pins the operator-side metric-name contract so the
    emission code in P10.14.2-FU1 can be unit-tested against
    an external source of truth, and so operators have
    something to import on day one.
  - Conventions captured in `dashboards/README.md` so future
    dashboards stay consistent (every panel references
    `${DS_PROMETHEUS}`, no embedded alert rules, no per-tenant
    JSON splits, no SLO panels until error-budget metrics are
    in the contract).
  - JSON validates via `jq . dashboards/grafana/*.json`.
    Tested by parsing in pre-commit; CI smoke is deferred to
    P10.14.2-FU1 where it can also assert that
    `/metrics` actually emits the named series.

- DONE P10.14.2-FU1 (Medium — v1.x) `/metrics` endpoint
  emission in `agentflow-server` (slice 1: workflow series)
  - Landed end-to-end for the three workflow-event-derived
    series the Grafana overview dashboard renders first:
    `agentflow_workflow_completed_total{status}` (counter,
    `status ∈ {succeeded, failed, cancelled}`),
    `agentflow_workflow_duration_seconds` (histogram, buckets
    0.1s … 10min), and `agentflow_nodes_failed_total{node_type}`
    (counter, labelled by `node_id` until a future event-payload
    extension splits node_type from node_id — documented inline).
  - New module `agentflow-server/src/metrics.rs`:
    `init_recorder()` (idempotent, OnceLock-guarded so multi-
    `run()` callers don't panic), `render_text()` (Prometheus
    text snapshot), `observe_workflow_completion(status,
    duration_seconds)`, `observe_node_failure(Option<&str>)`,
    plus `names::` constants pinning the exact wire strings
    the dashboard JSON queries against.
  - New `GET /metrics` route mounted on the `health`
    sub-router (no auth, same convention as `/health`).
    Returns `text/plain; version=0.0.4; charset=utf-8`.
  - `WorkflowEventListener::on_event` extended with a match
    arm that fires the counter/histogram on each terminal
    workflow event + the per-node-failure counter. No-op when
    no recorder is installed (the `metrics` facade silently
    drops calls).
  - `serve::run` installs the recorder once at boot; logs a
    warning instead of failing if install errors out so the
    rest of the gateway boots and `/metrics` returns an empty
    body (the documented behaviour).
  - **Deferred to follow-up TODOs** (opened below) for the
    other series in the dashboard's metric-name contract:
    cleanup sweep counters, worker fleet gauges, harness
    session gauges, scrape-time process inspectors.
  - 11 hermetic tests:
    - 5 in `metrics::tests` (name-constant pinning,
      empty-body-when-uninstalled, idempotency, counter +
      histogram emission, node_type label + `unknown`
      fallback).
    - 6 in `tests/metrics_endpoint.rs` (route returns OK +
      text/plain content-type, bypasses auth, emits the
      three contracted metric names, histogram bucket lines
      present, body content-type matches scrape-config
      expectation).
  - Deps: `metrics = "0.23"` + `metrics-exporter-prometheus
    = "0.15"` added to `agentflow-server`.
  - Docs: `docs/KUBERNETES_DEPLOYMENT.md` callout rewritten
    to mark slice 1 live + list the still-deferred series.
    `dashboards/README.md` "Current emission status"
    rewritten with a per-metric live/deferred matrix.

- DONE P10.14.2-FU2 (Low — v1.x) Wire retention sweep metrics
  - Landed. `cleanup_expired` now calls
    `metrics::observe_cleanup_sweep(report.dry_run,
    runs_deleted, events_deleted, artifacts_deleted)` at the
    end of every sweep. Dry-run sweeps are skipped (the panel
    is about actual deletions, not previews).
  - New `metrics::observe_cleanup_sweep` helper + three
    `CLEANUP_*_DELETED_TOTAL` constants in `metrics::names`
    pinning the wire strings the Grafana dashboard queries.
  - 1 new unit test (`observe_cleanup_sweep_increments_three_counters`)
    + 1 new integration test in `tests/metrics_endpoint.rs`
    (`metrics_endpoint_emits_cleanup_counters_after_observation`)
    that hits the real `/metrics` route and asserts the
    three counter names appear after observing a sweep.
  - Dashboard status: `dashboards/README.md` "Current emission
    status" matrix updated to mark all three counters ✅ live.
    `docs/KUBERNETES_DEPLOYMENT.md` callout updated from "3
    series live" to "6 series live."
  - Test-decision note: a dry-run-is-a-no-op unit test was
    attempted but races against the increment test under
    `cargo test`'s default parallelism. The no-op invariant
    is enforced by an obvious `if dry_run { return; }` branch
    + a Postgres integration test in `cleanup_route.rs` could
    cover it cleanly when a real sweep is exercised; left as
    a follow-up.

- DONE P10.14.2-FU3 (Low — v1.x) Wire worker fleet metrics
  - Landed. `AuthenticatedControlPlane` now emits the two
    gauges from its three mutation sites: `admit()` sets
    `agentflow_workers_admitted` to `state.admitted.len()`
    after every successful admission; `claim_task` sets
    `agentflow_worker_tasks_inflight{worker_id}` to the
    post-increment value after a task is claimed;
    `report_result` does the same after the decrement (and
    explicitly emits `0` for the "report without prior claim"
    branch so the panel doesn't carry a stale value).
  - New `metrics::observe_workers_admitted(count)` and
    `metrics::observe_worker_tasks_inflight(worker_id, count)`
    helpers + matching `WORKERS_ADMITTED` /
    `WORKER_TASKS_INFLIGHT` constants. Gauges are absolute
    (set-not-increment) so re-admissions and idempotent
    claims emit the same value without double-counting.
  - 2 new unit tests + 1 new integration test:
    - `observe_workers_admitted_emits_gauge`
    - `observe_worker_tasks_inflight_emits_per_worker_label`
    - `metrics_endpoint_emits_worker_fleet_gauges_after_admit_and_claim`
      — end-to-end through `AuthenticatedControlPlane` against
      an in-memory protocol, no live Postgres or gRPC needed.
  - Dashboard status: `dashboards/README.md` matrix → 8 ✅
    live. `docs/KUBERNETES_DEPLOYMENT.md` callout updated
    from "6 series live" to "8 series live."

- DONE P10.14.2-FU4 (Low — v1.x) Wire harness session metrics
  - Landed. The `/metrics` handler now runs scrape-time
    inspectors via a new `refresh_scrape_time_gauges(&state)`
    helper before rendering. Two gauges sourced from this
    path:
    1. `agentflow_harness_sessions_active{status}` —
       `SELECT status, COUNT(*) FROM harness_sessions
       GROUP BY status` against `state.db.read_pool()` (FU5
       reuse-ready). All four known statuses (`running`,
       `completed`, `failed`, `cancelled`) emit every
       scrape so a bucket that drops to 0 renders as 0
       instead of leaving a stale value; unknown statuses
       (DB / app drift) are ignored.
    2. `agentflow_harness_approvals_pending` —
       `PendingApprovalRegistry::pending_count()` (in-process
       mutex read, always succeeds).
  - Robustness: a DB-query failure inside the harness gauge
    refresh is logged at `tracing::debug` and swallowed; the
    scrape still returns 200 and the remaining metrics
    render. This is critical — a `/metrics` 500 would stop
    Prometheus from picking up *any* of the other 8 series.
    Pinned by the new
    `metrics_endpoint_scrape_time_handler_survives_unreachable_db`
    integration test.
  - New `observe_harness_sessions_active(status, count)` and
    `observe_harness_approvals_pending(count)` helpers +
    matching `HARNESS_SESSIONS_ACTIVE` /
    `HARNESS_APPROVALS_PENDING` constants. Both are
    set-not-increment gauges — the scrape-time refresh
    re-emits absolute counts every poll.
  - 4 new tests (2 unit + 2 integration):
    - `observe_harness_sessions_active_emits_per_status_label`
    - `observe_harness_approvals_pending_emits_gauge`
    - `metrics_endpoint_emits_harness_session_gauges_at_scrape_time`
    - `metrics_endpoint_scrape_time_handler_survives_unreachable_db`
  - Dashboard status: `dashboards/README.md` matrix → 10 ✅
    live. `docs/KUBERNETES_DEPLOYMENT.md` callout updated
    from "8 series live" to "10 series live" + documents
    the scrape-time pattern + the fail-soft contract.

- DONE P10.14.2-FU5 (Low — v1.x) Wire scrape-time process /
  state inspectors
  - Landed three of the four series:
    - `agentflow_health_status{component}` — emits
      `system=1` unconditionally (if the handler ran, the
      system is up by definition) and `database=1|0` from a
      cheap `SELECT 1` probe against the read pool.
    - `agentflow_memory_usage_bytes` — Linux reads
      `/proc/self/statm` and multiplies the resident-pages
      field by 4096. Non-Linux platforms emit `0` so dev
      macOS / Windows hosts render cleanly without an error
      surface in the panel (production targets are Linux
      99% of the time).
    - `agentflow_workflow_runs_active{tenant}` — single
      `SELECT tenant_id, COUNT(*) FROM runs WHERE status IN
      ('queued','running') GROUP BY tenant_id` against the
      read pool. Same fail-soft contract from FU4: a query
      failure logs at debug and skips the gauge but doesn't
      break the scrape.
  - 4th series `agentflow_state_size_bytes{run_id}` was
    deferred to `P10.14.2-FU6` at the time of this entry —
    that gauge is now live; see the FU6 closure below.
  - 5 new tests (3 unit + 2 integration). The
    `metrics_endpoint_emits_system_health_one_on_every_scrape`
    integration test pins the contract that the system
    component renders `1` on every successful scrape so a
    future refactor of `refresh_scrape_time_gauges` can't
    silently drop it.
  - Dashboard status: `dashboards/README.md` matrix → 13 ✅
    live + 1 ⏳ FU6 (state_size_bytes).
    `docs/KUBERNETES_DEPLOYMENT.md` callout rewritten to
    summarise the now-complete (modulo FU6) inspector
    surface.

- DONE P10.14.2-FU6 (Low — v1.x) Live-state size gauge
  - Landed end-to-end. `agentflow_state_size_bytes{run_id}` now
    renders in the "Memory & workflow state" panel; the
    dashboards/README.md status matrix shows 14 / 14 ✅ live.
  - **Core side** — new `agentflow-core::state_size` module ships
    the `StateSizeObserver` trait + `estimated_state_pool_bytes`
    helper. `FlowValue::estimated_size_bytes` measures Json
    variants via `serde_json::to_vec` (compact form); File/Url
    variants count only the path/url + mime-tag strings (the
    underlying blob lives elsewhere and is not part of the
    in-memory pool). `Flow` gains a private
    `state_size_observer: Option<Arc<dyn StateSizeObserver>>`
    field + `Flow::with_state_size_observer(...)` builder + a
    `notify_state_size(&state_pool)` helper called after every
    `state_pool.insert(...)` in both execution paths (serial +
    concurrent). Five insert sites instrumented; the observer
    sees one sample per node completion in either mode.
  - **Server side** — new `agentflow-server::live_state_registry`
    module ships `LiveStateRegistry` (`Arc<Mutex<HashMap<Uuid,
    u64>>>`). `LiveStateRegistry::observer_for(run_id)` returns
    an `Arc<dyn StateSizeObserver>` bound to a specific run id;
    every `observe(bytes)` call overwrites that run's entry.
    `snapshot()` copies out `(Uuid, u64)` pairs for the scrape
    path; `deregister(&run_id)` clears one entry. `AppState`
    gains a `live_state_registry: LiveStateRegistry` field
    (cheap to `Clone` — inner Arc-shared). `RunContext` gains
    an optional `live_state_registry` field threaded through
    from the route handler so the test-side `StubExecutor` can
    bypass it.
  - **Wiring** — `flow_execute` (in `runs.rs`) attaches the
    observer to the Flow before run, and explicitly deregisters
    after the terminal status update. `FlowRunExecutor::execute`
    also deregisters on the error path so cancelled / failed /
    build-failure runs don't leak label cardinality. The
    `refresh_scrape_time_gauges` helper iterates
    `state.live_state_registry.snapshot()` and emits
    `observe_state_size_bytes(run_id, bytes)` per entry — pure
    in-process read, no DB, no syscall.
  - **Metric contract** — new `names::STATE_SIZE_BYTES =
    "agentflow_state_size_bytes"`. New `observe_state_size_bytes
    (run_id: &str, bytes: u64)` helper, mirroring the
    `{run_id}`-labelled gauge convention from the FU3
    `{worker_id}` pattern.
  - **Tests (15 new across 3 layers)**:
    - 3 in `agentflow-core::value::tests`
      (`estimated_size_bytes_json_matches_compact_encoding`,
      `estimated_size_bytes_file_counts_path_and_mime_only`,
      `estimated_size_bytes_url_counts_url_and_mime_only`).
    - 4 in `agentflow-core::state_size::tests`
      (`estimated_state_pool_bytes_empty_pool_is_zero`,
      `estimated_state_pool_bytes_sums_keys_and_values`,
      `estimated_state_pool_bytes_skips_error_results_payloads`,
      `observer_trait_is_object_safe_and_receives_samples`).
    - 3 in `agentflow-core/tests/state_size_observer_tests.rs`
      (`state_size_observer_fires_per_node_in_serial_mode`,
      `state_size_observer_fires_per_node_in_concurrent_mode`,
      `flow_without_observer_runs_unchanged`).
    - 6 in `agentflow-server::live_state_registry::tests`
      (`snapshot_starts_empty`, `observer_records_under_its_run_id`,
      `multiple_observers_isolate_per_run_id`,
      `observe_overwrites_prior_sample_same_run_id`,
      `deregister_removes_entry_and_is_idempotent`,
      `cloned_registries_share_state`).
    - 1 in `agentflow-server::metrics::tests`
      (`observe_state_size_bytes_emits_per_run_id_label`).
    - 2 in `agentflow-server/tests/metrics_endpoint.rs`
      (`metrics_endpoint_emits_state_size_gauge_per_active_run_id`,
      `metrics_endpoint_state_size_gauge_drops_after_deregister`).
  - **Verification**: `cargo clippy -p agentflow-core
    -p agentflow-server --tests -- -D warnings` clean;
    `cargo test -p agentflow-core --lib` 139/139;
    `cargo test -p agentflow-core --test state_size_observer_tests`
    3/3; `cargo test -p agentflow-server --lib` 167/167;
    `cargo test -p agentflow-server --test metrics_endpoint`
    14/14. `docs/KUBERNETES_DEPLOYMENT.md` and
    `dashboards/README.md` updated from "13 series live + 1 ⏳"
    to "14 series live."

### P10.15 — agentflow-db (B+)

- DONE P10.15.1 (Medium — v1.x) Real backup/restore implementation
  - Landed: `agentflow backup --output <path>` orchestrates
    `pg_dump --format=custom` + `tar -czf` of the 5 filesystem
    state surfaces (run_dir, trace_dir, marketplace cache,
    skills, plugins) into one bundle directory with a versioned
    `manifest.json` (`agentflow.backup/1`).
  - Flags: `--output` (required), `--database-url`, `--include`
    (repeatable, aliases like `runs`/`database`/`traces`
    accepted), `--dry-run`, `--force`,
    `--format text|json|json-envelope` (canonical
    `agentflow.cli/1`).
  - Failure model: missing source dir is `skipped` (not failed);
    missing `pg_dump`/`tar` on PATH is `failed` with a
    package-manager install hint. Exit code `2` when any step
    failed.
  - Hermetic test surface: 12 unit tests in
    `commands::backup::tests` cover include parsing + aliases,
    URL password redaction, output-dir prep refuse/force/create/
    dry-run paths, and end-to-end dry-run behavior (full +
    subset + DB-only-without-URL skip path). Postgres / tar
    never invoked, so CI is fast and portable.
  - Docs: `docs/SERVER_BACKUP_RESTORE.md` gains a P10.15.1
    section with the flag reference, output layout, failure
    handling, and a pointer to the manifest version.
  - Restore wrap is **not** in scope; the manifest shape is the
    contract a future `agentflow restore --input <path>` would
    consume.

- DONE P10.15.2 (Low — v1.x) Read-replica support
  - Landed end-to-end. `Database` gains `read_pool: Option<PgPool>`
    + `Database::read_pool()` helper that falls back to the
    primary when no replica is configured. New
    `Database::connect_with_replica` /
    `connect_and_migrate_with_replica` constructors take the
    primary URL + replica URL + per-pool connection caps;
    migrations always run against the primary so DDL never
    races the replica.
  - Every Pg*Repo struct now carries both `pool` (write) and
    `read_pool` (read); a python pass routed 12 `SELECT`-shaped
    `fetch_*(&self.pool)` sites to `&self.read_pool` while
    leaving every `INSERT...RETURNING` / `UPDATE` / `DELETE` on
    `&self.pool`. New `Repositories::from_pools(write, read)`
    constructor + `Repositories::from_database(&db)` bridge
    pick the right pool per side; `from_pool(pool)` stays as a
    backwards-compat shim that uses the same pool for both
    sides.
  - `agentflow-server::AppState::new` now goes through
    `Repositories::from_database` so the moment an operator
    sets `AGENTFLOW_DATABASE_READ_URL` (or the new
    `agentflow serve --database-read-url` CLI flag), reads
    automatically route to the replica.
  - CLI plumbing: `agentflow serve` gains `--database-read-url
    <URL>` (default env `AGENTFLOW_DATABASE_READ_URL`); the
    flag forwards through to `agentflow-server` via env-var
    passthrough; the server binary reads the env directly in
    `build_config_from_env`.
  - 6 hermetic unit tests in `agentflow-db`:
    `database::tests::read_pool_falls_back_to_primary_when_not_configured`,
    `database::tests::read_pool_returns_replica_when_configured`,
    `repo::tests::from_pool_uses_same_pool_for_reads_and_writes`,
    `repo::tests::from_pools_routes_separate_pools_to_every_repo`
    (all 9 repos populated correctly),
    `repo::tests::from_database_threads_replica_into_repos_when_set`,
    `repo::tests::from_database_falls_back_to_primary_when_no_replica`.
    All use `PgPoolOptions::connect_lazy` so no live Postgres
    is required — `cargo test -p agentflow-db --lib` runs in
    under a second.
  - Backwards-compat invariants: every existing
    `Database { pool }` initializer in test files (2 sites)
    fixed up to `Database { pool, read_pool: None }`.
    `Repositories::from_pool(pool)` stays as the single-arg
    convenience constructor; `AppState::new` continues to
    accept a bare `Database`.
  - Replication-lag caveat documented in
    `docs/DEPLOYMENT.md` "Read-replica routing (P10.15.2)":
    write-then-immediately-read clients may observe pre-write
    state; cleanup sweep + run-row creation + harness session
    creation all read+write through the primary in the same
    call so they're unaffected.

### P10.16 — agentflow-worker (B)

- DONE P10.16.1 (Medium — v1.x) Signed-JWT identity flavour for
  worker admission (P5.5 deferred)
  - Landed. New `agentflow-server/src/scheduler/jwt.rs` ships
    `JwtPolicy` (issuer / audience / key pool / leeway),
    `JwtVerificationKey::{Hs256, Rs256}`, `WorkerJwtClaims` (with
    a tolerant `aud` deserializer that accepts string OR array
    per RFC §4.1.3), and `verify_worker_jwt[_at]` with strict
    `iss` / `aud` / `sub` / `exp` / `nbf` validation and
    operator-actionable error variants (`IssuerMismatch`,
    `AudienceMismatch`, `SubjectMismatch`, `Expired`,
    `NotYetValid`, `SignatureMismatch`, `Malformed`, `NoKeys`).
  - `WorkerAdmissionPolicy` gains `jwt: Option<JwtPolicy>` +
    `jwt_workers: HashSet<WorkerId>`. PSK takes precedence over
    JWT when a worker is misconfigured into both sets so a
    fat-fingered config can't silently downgrade auth. Workers
    in neither set stay anonymous (existing behavior).
  - `AdmissionError::InvalidCredential` extended with
    `reason: String` so the verifier-specific message
    (`"psk did not match any rotation entry"` / JWT verify
    error `Display`) reaches `tonic::Status::permission_denied`.
    No external consumers in the workspace; experimental tier
    per `docs/STABILITY.md`.
  - Key rotation: append a new `JwtVerificationKey` to the
    `JwtPolicy.keys` pool, flip the IdP, drop the old one. The
    verifier tries each key in order; first that succeeds wins.
    Mirrors the existing PSK overlap-add-then-remove pattern.
  - Tests: 14 unit tests in `commands::backup::tests` →
    `scheduler::jwt::tests` (HS256 happy path, empty key pool,
    signature mismatch, issuer/aud/sub mismatch with
    expected-vs-actual error fields, expired-after-leeway vs
    just-expired-within-leeway, nbf-in-future, key rotation
    pool, multi-aud string-vs-array deserialization,
    malformed-token surfaced cleanly) plus 7 new tests in
    `scheduler::admission::tests::jwt_flavor` covering the
    policy-layer routing (valid token admitted, missing
    credential, wrong subject, expired token, jwt_workers
    without `jwt` is config error, PSK-takes-precedence,
    anonymous workers still anonymous when JWT policy is
    set). All hermetic — no IdP / clock dependency, `now`
    injection lets test timestamps be deterministic.
  - Docs: `docs/DISTRIBUTED.md` "Worker Admission" section
    extended with the JWT knobs in the policy table + a
    dedicated "JWT identity flow (P10.16.1)" subsection +
    HS256-vs-RS256 guidance. `docs/STABILITY.md` row updated
    to list `JwtPolicy` and note the `InvalidCredential.reason`
    additive field. `docs/ROADMAP_v2.md` Theme E marks the
    decision closed.
  - Dep: `jsonwebtoken = "9.3"` added to
    `agentflow-server/Cargo.toml`.
  - gRPC-metadata propagation of admission tokens is still
    deferred to the broader auth story (separate TODO).

- DONE P10.16.2 (Low — v1.x) Worker pool admission heuristics
  (foundation slice)
  - Landed capability-aware dispatch + locality preference end-
    to-end for the in-memory protocol. New types
    (`WorkerCapabilities`, `ClaimHints`), additive optional
    fields (`WorkerTask.node_type: Option<String>`,
    `WorkerHeartbeat.capabilities: WorkerCapabilities`), new
    trait method `WorkerProtocol::claim_task_with_hints` with a
    default impl falling through to `claim_task` (so the gRPC
    adapter keeps compiling without behavior change), and an
    `InMemoryWorkerProtocol` override that scans the queue in
    three passes: (1) same-run AND capability-accepting,
    (2) capability-accepting regardless of run, (3) FIFO.
    Locality cache is per-worker, in-memory, and tracks the
    most-recently-claimed `run_id` so a worker without an
    explicit `locality_run_id` still gets warm-cache
    continuity. `WorkerControlPlane::claim_task_with_hints`
    forwards to the protocol and updates the run snapshot the
    same way `claim_task` does.
  - 9 hermetic unit tests in `scheduler::tests`: capability
    default accepts everything, restricted set filters out
    unmatched types, restricted set still accepts untagged
    tasks (additive upgrade), `claim_task_with_hints` skips
    unmatched-capability tasks, locality hint beats FIFO,
    locality with no match falls back to FIFO, cached
    last-claimed run biases subsequent claims, combined
    capability+locality picks the warmest matching task, and
    the control-plane wrapper still increments
    `running_tasks` on the run snapshot.
  - **Wire-extension status:** the in-memory protocol gets
    full capability + locality dispatch; the gRPC adapter is
    one follow-up away. `pb::ClaimTaskRequest` /
    `pb::HeartbeatRequest` don't carry the new fields yet, so
    workers talking gRPC effectively claim with "no hints"
    and get pre-P10.16.2 FIFO. Tracked as
    `P10.16.2-FU1` below. The trait surface stays
    forward-compatible so the wire-extension is purely
    additive.
  - Static `max_workers` + `max_concurrent_tasks_per_worker`
    caps from P5.5 remain unchanged; capability + locality
    are additive on top.
  - Docs: `docs/DISTRIBUTED.md` gains a "Worker Capability +
    Locality Hints (P10.16.2)" section between admission and
    resource limits; `docs/STABILITY.md` row updated to list
    `WorkerCapabilities` / `ClaimHints` and note the new
    optional fields.

- DONE P10.16.2-FU1 (Low — v1.x) Plumb capability + locality
  hints across the gRPC wire
  - Landed end-to-end. `pb::WorkerTask` gained `node_type:
    string` (tag 6); `pb::ClaimTaskRequest` gained
    `accepted_node_types: repeated string` (tag 2) +
    `locality_run_id: string` (tag 3); `pb::HeartbeatRequest`
    gained `accepted_node_types: repeated string` (tag 5). All
    four fields are wire-additive — pre-FU1 workers (which
    never set them) encode as empty values, which the server
    decodes as "no hints / untagged task" preserving the
    pre-FU1 FIFO behavior exactly.
  - `worker_task_to_proto` / `worker_task_from_proto` round-trip
    `node_type` with the empty-string ↔ None mapping critical
    for the "untagged-task-always-accepted" invariant.
  - New `claim_hints_from_proto` helper decodes the
    `accepted_node_types` + `locality_run_id` pair into a
    `ClaimHints`, with proper validation of malformed UUID
    locality hints (surfaced as
    `tonic::Status::invalid_argument`).
  - `worker_heartbeat_to_proto` / `worker_heartbeat_from_proto`
    carry the per-heartbeat capability advertisement.
  - **Both** gRPC service impls (`GrpcWorkerService` and
    `WorkerControlPlane`'s tonic adapter) now route through
    `protocol.claim_task_with_hints` so the capability filter
    actually filters when a worker advertises capabilities.
    `GrpcWorkerProtocol` (the client) gained an explicit
    `claim_task_with_hints` impl; `claim_task` becomes a thin
    shim that delegates with `ClaimHints::none()`.
  - `agentflow-worker::WorkerConfig` gained
    `capabilities: WorkerCapabilities` + `with_capabilities`
    builder. `run_once` now sends the configured capabilities
    on every heartbeat AND attaches them to the claim hints,
    so distributed workers can declare which node types they
    accept and the queue scan skips work they can't run.
  - 7 hermetic unit tests in `scheduler::grpc::hint_proto_tests`:
    `worker_task_round_trip_preserves_node_type`,
    `worker_task_round_trip_preserves_untagged`,
    `claim_hints_round_trip_carries_capabilities_and_locality`,
    `claim_hints_from_proto_default_means_no_hints`,
    `claim_hints_from_proto_rejects_malformed_locality_uuid`,
    `heartbeat_round_trip_preserves_capabilities`,
    `heartbeat_pre_fu1_default_decodes_as_any_capability`.
    Combined with the policy-level tests from P10.16.2
    (`scheduler::tests`, 9 tests), the capability + locality
    surface is now covered at both the protocol and the wire
    level.
  - `docs/DISTRIBUTED.md` "Wire-extension status" subsection
    updated to mark FU1 closed, with a wire-shape table
    showing every new field's pb type + pre-FU1 default.

### P10.17 — agentflow-ui (B → "operator dashboard")

- DONE P10.17.1 (HIGH — v1.x) Decide product positioning
  - **Committed to debugger-focused.** Operator dashboard
    features (cost aggregation, retry-rate trends, policy-
    decision summary, worker fleet utilization) are explicitly
    out of scope; Prometheus + Grafana + BI tools cover those
    better, and the server already exposes Prometheus metrics
    for scraping. The CLI + trace replay remain the headless
    surface — `RoadMap.md` already pinned "Web UI should not be
    required for headless operation"; this commit makes the bar
    a first-class doc section instead of a one-line aside.
  - `docs/WEB_UI.md` gains a "Product positioning" section near
    the top with: the committed direction, an in-scope / out-of-
    scope table, the rationale (maintenance budget + better
    alternatives + headless line), the v1.1 additive items that
    stay inside the boundary (Harness session replay UI, trace
    compare polish, long-run perf inc. P10.17.3, prefs API
    wiring P10.17.2, Playwright e2e P10.17.4), and concrete
    alternative-tool pointers for each out-of-scope category.
    Last paragraph names the contributor workflow: "ask if it
    fits the in-scope column; if no, write a v2 RFC".
  - `RoadMap.md::Web UI Productization` updated: the existing
    one-liner now points at the canonical doc section so future
    contributors land on the in/out table instead of inferring
    boundaries from the prose.

- DONE P10.17.2 (Medium — v1.x) Preference UI wiring to P6.4 API
  - Landed the helper + hook + proof-of-pattern wiring; the
    canonical sync contract is now live for the run-console
    tenant id. New files:
    - `agentflow-ui/src/preferences.ts` — pure helper module
      with `STATIC_KEY_MAP` (7 syncable local keys → server
      keys), `serverKeyForLocal` / `localKeyForServer` (with
      dynamic per-run-id event-filter prefix handling),
      `loadServerPreferences` / `saveServerPreferences`,
      `tenantHeaders`, and `PreferenceWriteQueue` (500 ms
      debounce + last-write-wins per key).
    - `agentflow-ui/src/preferences.test.ts` — 28 PASS in the
      same bun/tsc-runnable pattern as `eventFilter.test.ts`
      (key mapping in both directions, dropped-unknown-keys,
      static + dynamic event-filter prefixes, fetcher contract
      including `X-Agentflow-Tenant`, queue collapse / cancel
      / flush-now).
    - `agentflow-ui/src/usePreferenceSync.ts` — React hook that
      GETs once per `(apiToken, tenant)` pair, exposes
      `{ serverPrefs, syncToServer }`, debounces PUTs via the
      queue, and flushes pending writes on unmount so an
      operator's last edit doesn't get lost between
      navigations.
  - **Scope split**: only `agentflow.ui.tenantId` (run console)
    is end-to-end-wired into the UI in this commit (load
    overlay + write sync). The other 6 syncable keys are
    *mapped* in the helper but their components still write to
    localStorage only — replicating the 3-line pattern in each
    component is mechanical and tracked as a follow-up *inside*
    the doc table. Splitting it this way kept the diff to
    main.tsx small (one component, ~12 lines added) so the
    pattern is reviewable.
  - Explicitly NOT synced (security / size / machine-specific):
    api token, workflow YAML drafts, harness user_input prompt,
    harness workspace_root path. Reasons are pinned in
    `docs/WEB_UI.md` + tests.
  - `docs/WEB_UI.md` gains a "Durable preferences (P10.17.2)"
    section with the synced-vs-never-synced tables + wire-shape
    contract notes (regex constraint, 16 KiB cap, token-shape
    rejection). `npm test` (bun-driven) green on 28 helper
    tests; `npx tsc --noEmit` clean; `cargo test
    -p agentflow-server` all 82 tests green after the rebuilt
    `dist/` (the server embeds the bundle via `include_str!`).

- DONE P10.17.3 (Medium — v1.x) Server-side `?filter=` pre-filter
  for very long runs
  - Landed: new `agentflow-server::events_filter` module mirrors
    the client-side `eventFilter.ts` grammar (`kind=` / `kind!=`
    / `kind~` / `step<op>N` joined by case-insensitive `AND`).
    Wired into `GET /v1/runs/{id}/events/history` via a new
    `filter` query param. Empty / absent → no filter (fast
    path); parse errors → 400 with the single-line parser
    message so the UI's 400-fallback can pattern-match.
  - The parser is strict on the server side (responds 400) but
    lenient on the client (surfaces error inline) — same
    behaviour the docs already promised. Both implementations
    use the same surrounding-whitespace AND split rule, so
    `kind=foo_AND_bar` stays as one clause whose value contains
    `AND` instead of getting mis-split.
  - UI: `RunConsole` history fetch passes the operator's saved
    filter expression (read from localStorage on the
    runId-changed effect) on initial run attach. 400 responses
    silently re-fetch without the filter — the inline parse
    error from `compileFilter` is already what the operator
    actually sees and edits. Client-side filter stays active as
    a defensive for live SSE events (which aren't
    server-pre-filtered) and for filter changes after the
    initial fetch (no re-fetch on filter edit; the saved value
    drives the wire reduction).
  - Tests: 21 unit in `events_filter::tests` (every clause
    shape, case-insensitivity, AND-inside-value non-split,
    every parse error, every operator's matches() behaviour
    including the "events without step_index get excluded from
    step clauses" rule) + 4 self-skipping route integration
    tests in `tests/events_filter_route.rs` (kind-contains
    happy path, after_seq+filter compose, 400-on-bad-expr,
    empty-param no-op). `cargo clippy -p agentflow-server
    --tests -- -D warnings` clean; `npx tsc --noEmit` clean
    after the UI patch; `npm run build` rebuilt the embedded
    `dist/`.
  - `docs/WEB_UI.md` Architecture section gains a `?filter=`
    line under the dependency list.

- DONE P10.17.4 (Low — v1.x) Playwright suite in CI
  - Picked option (a) — new `.github/workflows/ui-e2e.yml` job
    with `workflow_dispatch` + nightly schedule at 10:30 UTC.
    GitHub Actions `services: postgres:16-alpine` provides the
    DB (no docker-compose needed); the server boots in the
    background of the job with a 30s `/ui` readiness probe.
    Playwright + Chromium installed via `npm run e2e:install`;
    `npx playwright test` runs the 6 specs across the 2
    existing files (`runs-new.spec.ts` + `harness-sessions.spec.ts`).
    Failure path uploads `playwright-report/` + traces as a
    14-day artifact; the JUnit XML feeds GitHub's test
    parser.
  - **NOT** in `quality.yml::release-gate.needs` — explicit
    decision pinned in `agentflow-ui/e2e/README.md::Why not
    PR-gated`. The two-spec coverage doesn't justify the
    build + browser-install + flakiness tax on every PR;
    nightly catches regressions between releases. Promotion
    is a single-edit change if a real regression slips
    through later.
  - `@playwright/test` promoted from "intentionally optional
    dev dep" (per the existing comment in `runs-new.spec.ts`)
    to a real `devDependencies` entry, locked at `^1.49.0`.
    New `playwright.config.ts` uses the same `globalThis`
    cast pattern as `eventFilter.test.ts` /
    `preferences.test.ts` so it doesn't need `@types/node`.
    Config picks `retries: 1` + `workers: 1` under CI for
    cold-start absorb + DB transaction safety; `0/auto` for
    local devs.
  - Full operator + CI runbook in
    `agentflow-ui/e2e/README.md`: one-time setup, per-run
    flow, env knob table, CI artifact retrieval, the
    "adding a spec" checklist, and the "why not PR-gated"
    rationale so future contributors don't keep relitigating
    the gating question. `docs/WEB_UI.md::Verification`
    gains an "E2E (P10.17.4)" sub-section pointing at the
    runbook.
  - `npx tsc --noEmit` clean after the install;
    `npx playwright test --list` enumerates all 6 specs
    across both files.

### P10.18 — xtask (A-)

No active gaps. Future opportunities:

- DONE P10.18.1 (Stretch) `cargo xtask refresh-live-models`
  - Closed jointly with P10.3.4 — see that entry for the full
    write-up. Cross-referenced TODOs were the same task.
- DONE P10.18.2 (Stretch) `cargo xtask check-changelog`
  - New subcommand at `xtask/src/main.rs::check_changelog_from_args`.
    Args: `cargo xtask check-changelog [BASE_REF]` (default
    `origin/main`). Behaviour:
    1. `git diff --name-only BASE...HEAD` to enumerate the
       branch's touched files.
    2. Classify every path through `is_trivial_changelog_path`
       (docs/ + *.md + Cargo.lock / package-lock.json +
       .gitignore + .github/workflows/ + tests/ + **/fixtures/
       + *.test.ts / *.test.rs). Trivial-only changes → PASS.
    3. Else: PASS when CHANGELOG.md is touched OR any commit
       body in BASE..HEAD contains `chore(skip-changelog)`.
    4. Else: FAIL listing the non-trivial paths + the
       skip-marker escape hatch.
  - Tests: 5 new in `check_changelog_tests` (trivial-path
    classifier covering each prefix/suffix family; the 4
    outcome paths each with a real `tempfile + git init`
    fixture). The classifier test alone catches a regression
    that narrows the trivial set, which is the most
    operator-impactful break-mode (suddenly more PRs need a
    CHANGELOG bump).
  - **Not** wired into `quality.yml` today — landing the
    xtask first lets contributors run it locally and confirms
    the heuristic against real PRs before it gates anything.
    `print_usage` text + module rustdoc document the contract.

### P10.19 — Cross-crate / workspace level

- DONE P10.19.1 (HIGH — pre-GA) WASM plugin runtime evaluation
  - 1-pager landed at `docs/WASM_PLUGIN_EVALUATION.md`. The
    narrowed wasmtime-vs-wasmer-vs-extism comparison concludes
    that **if** we ever adopt WASM, wasmtime + WIT + WASI 0.2 is
    the right runtime (industry default in 2026; component-model
    + WASI 0.2 stable async is the only path that matches our
    `AsyncNode::execute` shape without abstraction loss). wasmer
    is rejected on ecosystem-bifurcation grounds; extism is
    rejected as the wrong abstraction tier (bytes-only, not
    typed nodes participating in a `FlowValue` dataflow).
  - **Decision: push to v2.0.** The subprocess runtime is stable
    and the WASM win (sub-ms cold start, single-binary plugin
    distribution, finer-grained capability sandbox) doesn't
    solve any current user complaint — the 50-200 ms subprocess
    cold start is dominated by the first LLM call's TCP
    handshake in any realistic workflow. Pre-GA opportunity
    cost (~6-8 person-weeks for WIT design + 3 polyglot
    examples + CI surface) is better spent on the remaining
    operator-facing HIGH items (P10.0.x). The
    `PluginRuntime::Wasm` enum variant stays in
    `agentflow-core/src/plugin/manifest.rs` as a
    forward-compatible reservation; v2.0 wires the real host.
  - Re-evaluation triggers (any one is sufficient): concrete
    latency complaint, polyglot-plugin demand from a non-Rust
    contributor, single-binary distribution complaint, or a
    peer project (Helix/Zed/Lapce) shipping a WASM
    plugin ecosystem that creates an ergonomics gap.

- DONE P10.19.2 (Medium — v1.x) Workspace-wide perf regression
  detection
  - Landed as a new `cargo xtask test-gate` subcommand, sibling
    to `bench-gate`. Same architectural shape (baseline JSON +
    threshold + compare report); different metric: per-crate
    `cargo test -p <crate> --all-targets` wall-clock instead of
    criterion microbench medians. Threshold defaults to **1.5×**
    (looser than bench-gate's 1.25× because test wall-clock
    captures both incremental compile and execution and `cargo
    test` has no warm-up / measurement-time knobs to dampen
    variance).
  - **Modes**:
    - Default: walks `workspace.members` (minus `xtask` itself),
      runs `cargo test -p <crate> --all-targets --quiet` per
      crate, times each with `Instant::now()`, reads the host-
      specific baseline file, fails when any crate's
      `current_ns / baseline_ns >= threshold`.
    - `--update`: same sweep but writes the timings to the
      baseline path instead of comparing. Stamps `host.captured_at`
      (UTC via `date -u +%F`) + `host.rustc` (`rustc --version`)
      automatically.
    - `--input <path>`: pure-data comparison flow — read a
      pre-captured "current" JSON, no cargo invocation. Useful
      for CI two-stage flows where capture and gate live in
      different jobs. Mutually exclusive with `--update`.
  - **Flags**: `--baseline <path>` (override default), `--threshold
    <ratio>` (default 1.5; must be > 1.0 + finite),
    `--allow-missing` (don't error when baseline has entries with
    no matching current measurement — mirrors bench-gate),
    `--include <crate>` / `--exclude <crate>` (repeatable; the
    intersection rules match the obvious union/difference
    semantics).
  - **Baseline files** live at
    `benches/baselines/test-timings/<host>.json` (parallel
    directory to the criterion baselines, same `<host>.json`
    naming). A `README.md` in that directory documents schema,
    capture flow, threshold rationale, and the "first-time
    capture" bootstrap. **No initial baseline files ship in this
    commit** — running `cargo test` across the full workspace
    takes 5–15 minutes per host and the numbers vary per
    machine, so the first operator to capture on a given host
    lands the file as a separate diff. Plain `test-gate` (no
    `--update`) fails fast with an actionable error pointing at
    `--update` until a baseline exists.
  - **Schema**: `{ host: {id, machine?, arch?, os?, rustc?,
    captured_at?}, notes: [..], timings: { <crate>: {
    wall_clock_ns: u128, test_count: Option<u64> } } }`. The
    `test_count` field is best-effort, parsed from the
    `test result: ok. N passed; ...` summary lines and summed
    across binaries (libtest emits one summary per binary
    inside a single `cargo test` invocation). `None` when no
    summary appears (compile failure, harness disabled).
  - **Tests (16 new in `xtask::test_gate_tests`)**: 4 pure
    comparator boundary tests (at-threshold / faster / zero-
    baseline / missing-keys), 3 test-count parser tests
    (multi-binary sum / no-summary / format drift), 1 serde
    round-trip pinning the schema, 3 crate-selection tests
    (xtask excluded / include filter / exclude filter), 1
    write+read round-trip, 1 threshold validation, 1
    `--update` vs `--input` mutual-exclusion check, 2
    end-to-end pure-data tests driving `test_gate_from_args`
    through `--input` (happy + failing). All hermetic — no
    cargo invocation needed at test time. `cargo test -p
    xtask test_gate` 16/16 green; `cargo clippy -p xtask
    --tests -- -D warnings` clean.
  - **Not in scope** (separate follow-up): wiring `test-gate
    --allow-missing` into `quality.yml` after a baseline file
    lands. Per the README rationale, landing the gate first lets
    contributors run it locally for a release before it gates
    CI — same staged rollout the bench-gate followed.

- DONE P10.19.3 (Low — Stretch) Centralized `docs/ROADMAP_v2.md`
  for post-v1.0 direction
  - Created `docs/ROADMAP_v2.md` as the single source of truth
    for "what's after v1.0 GA". Organised into 10 themes
    (A–J): LLM/provider expansion, Memory/RAG, Server platform,
    Web UI (debugger-scoped per P10.17.1), Distributed,
    Harness H6, Plugin runtime (WASM), Perf, Ops tooling,
    Docs/contributor experience. Each remaining v1.x bullet
    here gets a back-reference (`P10.X`) so the audit trail
    is one-step.
  - Doc opens with `Status` (not binding; staging ground) +
    `v1 → v2 inflection` (the maintenance bar tightens
    post-GA). Closes with `Non-goals for v2` (carries forward
    the v1 non-goals + adds operator-dashboard Web UI per
    P10.17.1 + UI-as-headless-requirement) and a `How to
    promote` workflow (open a `P11.x` TODO entry →
    backreference the theme letter → leave a stub here).
  - `RoadMap.md::Later Tracks` gains an inline pointer at the
    top of the section directing future contributors to the
    v2 doc; the existing prose is preserved as historical
    rationale.

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
