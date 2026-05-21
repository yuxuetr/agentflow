# AgentFlow v1.0.0-rc.1 Release Notes

**Status**: Release candidate. The `## Production Deployment Checklist`
section is the v1.0 deployment runbook (stable independent of the tag
cut, owned by P7.4-FU4). The `## What's New`, `## Breaking Changes`,
and `## Known Issues` sections below summarise the 531 commits between
`v0.2.0` and this tag.

**Linked rehearsal**: `docs/RELEASE_NOTES_DRESS_REHEARSAL.md`.

**Crate versions at tag time**: the workspace's 15 publishable crates
remain on their independent pre-1.0 SemVer trajectories
(`agentflow-core 0.2.0`, `agentflow-rag 0.3.0-alpha`, etc.). The
`v1.0.0-rc.1` git tag is a **project-milestone marker** — "the
operator-facing stability surfaces are RC-ready" — not a coordinated
crate version bump. Coordinated 1.0 crate publishing is a separate
follow-up after RC-period feedback.

---

## Production Deployment Checklist

This checklist closes finding `F4` from the v1.0.0-rc.1 dress
rehearsal: `agentflow doctor --profile production` returns
`status: warning` on a fresh host because nothing under
`~/.agentflow/` exists yet and `AGENTFLOW_API_TOKEN` is unset.
None of those are code defects; they are operator-runbook gaps. Walk
through every step below before swinging traffic at a freshly
provisioned production host.

The reference deploy shape is `docker-compose.yml` at the repo root
(Postgres + `agentflow-server`). Bare-metal / Kubernetes deployments
follow the same env-var contract — only the orchestration layer
changes.

### 1. Pick a security profile and wire it through the environment

`AGENTFLOW_SECURITY_PROFILE` selects coarse runtime posture (defined
in `agentflow-tools/src/security_profile.rs`). Production deployments
**must** set:

```bash
export AGENTFLOW_SECURITY_PROFILE=production
```

What this turns on (high level — see `docs/TOOL_PERMISSIONS.md` for
the full matrix):

- Server auth fail-closed: `AGENTFLOW_API_TOKEN` becomes mandatory.
  Missing token aborts startup with a clear `AGENTFLOW_API_TOKEN is
  required when AGENTFLOW_SECURITY_PROFILE is 'production'` error.
- CORS defaults to deny-by-default; expose origins explicitly via
  `--cors-origins` (or the equivalent `ServeConfig` field).
- Plugin install / spawn requires a sandboxed runtime; the
  `--allow-unsandboxed-plugin` opt-out is rejected unconditionally.
- Marketplace install requires a verified signature
  (`--signed` / strict policy).
- HTTP tool SSRF protections (block private IPs / loopback) are
  enforced.
- Tool admission falls back to deny-on-no-match (P1.9).

`dev` is intentionally permissive (single-developer fast-loop) and
`local` is the default for single-user CLI use. Neither belongs in a
production deployment.

### 2. Provision the API auth token via your secret manager

Generate a random opaque token (≥ 32 characters of CSPRNG output is
the recommended floor) and inject it as `AGENTFLOW_API_TOKEN`
through your platform's secret manager. Do **not** check the token
into the repo, the deployment manifest, or shell history.

Examples (substitute the right backend for your platform):

```bash
# Kubernetes
kubectl create secret generic agentflow-api-token \
  --from-literal=AGENTFLOW_API_TOKEN="$(openssl rand -hex 32)"

# systemd
sudo systemctl edit agentflow-server.service
# add under [Service]:
#   EnvironmentFile=/etc/agentflow/api-token.env
# where the file is 0600 root:agentflow and contains
#   AGENTFLOW_API_TOKEN=...

# docker compose
# docker-compose.yml already exposes the slot; uncomment and replace
# the placeholder, then source the value from your secret manager
# rather than committing it.
```

All `/v1/*` requests then require `Authorization: Bearer <token>`.
`GET /health/live` and `GET /health/ready` stay open so an
orchestrator can probe liveness without a token.

### 3. Pre-provision storage directories

`agentflow-server` writes to three storage roots. In `production`
they should exist with appropriate ownership and free space **before**
the first `agentflow serve`. The server will auto-create them on
first write, but production posture rejects this advisory at
`doctor` time so a missing path doesn't quietly become a Tier-0
incident the first time a run lands.

Env-var contract (resolved by `agentflow-cli/src/commands/doctor.rs`
and `agentflow-server/src/serve.rs`):

| Env var | Default if unset | What lives here |
| --- | --- | --- |
| `AGENTFLOW_RUN_DIR` | `$HOME/.agentflow/runs` | Per-run artifact directory (one subdir per run UUID). Cleanup sweep (P2.2) reaps terminal runs past `run_dir_retention_days`. |
| `AGENTFLOW_TRACE_DIR` | `$HOME/.agentflow/traces` | JSONL trace storage and (when feature-enabled) SQLite trace storage. Consumed by `agentflow trace replay`. |
| `AGENTFLOW_MARKETPLACE_CACHE` | `$HOME/.agentflow/marketplace-cache` | Verified artifact cache for `agentflow marketplace install`. Backed-up separately from skill / plugin install roots. |
| `AGENTFLOW_SKILLS_DIR` | `$HOME/.agentflow/skills` | Installed skill manifests + assets. Read by `agentflow skill list` / `inspect` / `run`. |
| `AGENTFLOW_PLUGINS_DIR` | `$HOME/.agentflow/plugins` | Installed plugin manifests + binaries. Read by `agentflow plugin list` / `inspect`. |

Pre-provision example for a system user `agentflow`:

```bash
sudo install -d -o agentflow -g agentflow -m 0750 \
  /var/lib/agentflow/runs \
  /var/lib/agentflow/traces \
  /var/lib/agentflow/marketplace-cache \
  /var/lib/agentflow/skills \
  /var/lib/agentflow/plugins

cat <<'EOF' | sudo tee /etc/agentflow/agentflow-server.env
AGENTFLOW_SECURITY_PROFILE=production
AGENTFLOW_RUN_DIR=/var/lib/agentflow/runs
AGENTFLOW_TRACE_DIR=/var/lib/agentflow/traces
AGENTFLOW_MARKETPLACE_CACHE=/var/lib/agentflow/marketplace-cache
AGENTFLOW_SKILLS_DIR=/var/lib/agentflow/skills
AGENTFLOW_PLUGINS_DIR=/var/lib/agentflow/plugins
EOF
```

Backup / restore semantics for each of these surfaces is documented
in `docs/SERVER_BACKUP_RESTORE.md`.

### 4. Wire Postgres + run the migration

Provide `DATABASE_URL` pointing at a Postgres 14+ instance the
deployment owns end-to-end (no shared multi-tenant DB). `agentflow
serve` runs the embedded `sqlx::migrate!()` chain on first boot, so
no manual migration step is required for a fresh DB. For DR /
restored DBs, see `docs/SERVER_BACKUP_RESTORE.md` § "Postgres".

```bash
export DATABASE_URL=postgres://agentflow:<password>@db.internal:5432/agentflow
```

### 5. Verify with `agentflow doctor --profile production`

Run the structured diagnostic and confirm exit code `0`:

```bash
agentflow doctor --profile production --backup-check --format json
echo "exit=$?"
```

Expected:

- `exit = 0` (anything `>= 1` means at least one warning; `2` means
  a hard failure).
- `disk.run_dir.exists`, `disk.trace_dir.exists`,
  `disk.marketplace_cache.exists` all `true` with `writable: true`.
- `auth.api_token_set` is `true`.
- `security.profile` is `"production"`.
- `sandbox.enforcement` is `"enforcing"` on the target host (Linux
  seccomp or macOS sandbox-exec; the noop backend is **not**
  acceptable in production).

If any line surfaces a warning or failure, fix the gap, re-run
`doctor`, and **do not swing traffic** until the report exits `0`.
This is the same gate documented for the backup-restore validation
checklist in `docs/SERVER_BACKUP_RESTORE.md` § "First-stable-release
validation checklist".

### 6. (Optional) Reference: docker-compose smoke

The `docker-compose.yml` at the repo root is the reference deploy
shape — Postgres + `agentflow-server` on the same network, with
healthchecks plumbed for both. To smoke-test the image locally
before the production roll:

```bash
docker compose up -d
docker compose ps                   # both services -> (healthy)
curl -fsS http://localhost:3000/health/live    # -> 200 ok
curl -fsS http://localhost:3000/health/ready   # -> 200 ok
open       http://localhost:3000/ui            # SPA shell renders
```

The compose file leaves `AGENTFLOW_API_TOKEN` commented out for
local dev. **Do not deploy that shape to production untouched** —
swap the env block to source the token from your secret manager and
flip `AGENTFLOW_SECURITY_PROFILE` to `production` before promoting.

---

## Acceptance gate for this section

A fresh operator following the steps above on a clean VM:

1. `agentflow doctor --profile production --backup-check` exits `0`.
2. `agentflow serve --check --security-profile production` reports
   `readiness: ok`.
3. `agentflow serve` starts cleanly and serves `/health/ready` `200`.
4. An authenticated `POST /v1/runs` against the running gateway
   completes successfully (uses the provisioned token).

If any of these fail, the gap is a code or runbook bug — file it.

---

## What's New

The four-layer mental model (Execution / Capability / Agent /
Operations) reached operator-ready maturity. Highlights — see
`CHANGELOG.md` for the full per-PR list (531 commits since v0.2.0):

### Platform — server, gateway, observability

- **`/v1/runs` + SSE + Harness HTTP surface** (P-H.5 closed): Live
  HarnessExecutor + `LiveHarnessExecutor` + Postgres-backed harness
  session persistence; SSE backfill + `/events/history` JSON;
  `:resume` `rerun` / `append` modes.
- **Prometheus `/metrics` slice** (P10.14.2-FU1–FU6, 14 / 14 series live):
  workflow completion + duration histograms, node-failure counters,
  cleanup-sweep deletions, worker fleet gauges, harness session
  gauges, scrape-time process inspectors, **live-state size gauge**
  (per-run `Flow::state_pool` size via `LiveStateRegistry` +
  `StateSizeObserver`).
- **Per-run retention overrides** (P10.14.2): `POST /v1/runs` body
  accepts `retention_overrides: {events_days, artifacts_days}`;
  cleanup sweep uses `max(global, override)`.
- **Read-replica routing** (P10.15.2): `AGENTFLOW_DATABASE_READ_URL`
  + `--database-read-url`; `Database::read_pool()` falls back to
  primary; SELECT-shaped repo paths auto-route.
- **Worker pool admission heuristics** (P10.16.1 + FU1): JWT
  identity flavour (HS256 / RS256, key-rotation pool); capability +
  locality hints over the gRPC wire (`WorkerCapabilities` +
  `ClaimHints`); same-run-warm-cache continuity.

### Observability + perf gating

- **Hot-path criterion benches** (P10.1.1 + P10.2.1):
  `agentflow-core/benches/hot_paths.rs` covers FlowValue decode +
  checkpoint roundtrip (9 bench points); `agentflow-nodes/benches/
  node_latency.rs` covers template / conditional / file (10 bench
  points). Wired into the `bench-gate` baseline; CI gate runs all
  6 benches per PR at the default 1.25× threshold.
- **Test-suite-bloat gate** (P10.19.2): `cargo xtask test-gate`
  captures per-crate `cargo test` wall-clock, gates on 1.5×.
- **`agentflow agent replay --diff`** (P10.8.1): ReAct trace
  divergence diff (file-to-file, no LLM call). Step-order +
  tool-call + stop-reason + per-step token-delta dimensions.
- **`agentflow trace replay --speed`** (P10.10.2): Harness session
  replay with `1x` / `2x` / `inf` / `instant` pacing.

### LLM provider expansion

- **9 providers live in nightly CI** (N9 closed): OpenAI / Anthropic
  / Google / Moonshot / StepFun / GLM·Zhipu / DashScope·Alibaba /
  DeepSeek / MiniMax. `OpenAIProvider::with_client(...)` covers the
  6 OpenAI-compat vendors via the bundled `default_models.yml`.
- **`cargo xtask refresh-live-models`** (P10.3.4 / P10.18.1): pings
  every provider's `/models` endpoint, validates the live-test
  text-model default, suggests replacements on 404. Tolerant of
  Anthropic's rolling-alias-only convention via a dated-revision
  matcher.
- **Lenient `LLMConfig::validate()`** (P10.3.1): no longer
  fail-close on missing API keys; emits a warning and the
  ModelRegistry skips affected providers. New `validate_strict()`
  preserves the production-doctor path.
- **Provider-specific tokenisers** (P10.3.3 + FU1): `TokenCounter`
  trait + `TiktokenCounter` (BPE for OpenAI family + compat
  vendors); `ReActAgent` / `PlanExecuteAgent` now compact memory
  history against precise BPE counts on the families that share
  cl100k_base.

### Agent runtime + skills

- **Multi-agent supervisors** (N9): Handoff / Blackboard / Debate;
  `multi_agent` YAML node.
- **Hooks + approval pipeline** (P-H.2 closed): `HookedTool` wraps
  every registered tool; `Cli` / `AutoAllow` / `AutoDeny` approval
  providers; `Session` / `Run` scope caching; `DenyAndStop`
  short-circuit.
- **Background-task subsystem** (P-H.4): `TaskRuntime` + `TaskHandle`
  + 5 built-in tools (`task_create` / `_get` / `_list` / `_stop` /
  `_output`).
- **Per-tool `os_sandbox` override** (P10.4.1): `[[tools]]
  os_sandbox = true|false` lets heterogeneous-policy skills sandbox
  shell but not script (or vice versa). Inherits manifest-level
  default when absent.
- **Skill MCP-discovery cache** (P10.9.1): 24h-TTL persisted cache
  at `~/.agentflow/cache/skill_mcp_discovery.json`; ` --refresh-mcp
  -cache` / `--no-mcp-discovery` flags.

### Storage + memory

- **Encryption-at-rest for the preference layer** (P10.7.2):
  `AgeEncryptedPreferenceStore` wraps any `PreferenceStore` with
  X25519 age encryption; `age:v1:` marker prefix; rolling-alias-
  safe identity-file helpers.
- **`agentflow memory prune` CLI** (P10.7.1): retention-window
  pruning for `preference` + `entity_facts` layers; bare-integer
  durations rejected up front.
- **Live RAG eval per-chunk-size dimension** (P10.6.3): `agentflow
  rag eval --chunk-size N` chunks corpus → remaps chunk-ids back
  to source-doc-ids before scoring. `chunk_size: Option<usize>`
  persists in baseline JSON.
- **Pluggable retriever trait** (P10.6.1): in-tree `Bm25Eval`,
  `DenseEval`, `HybridEval` (Reciprocal Rank Fusion). CLI
  `--retriever {bm25,dense,hybrid}` + `--embedding-model`.

### Release engineering

- **Reproducible fresh-VM doctor smoke** (P10.0.5): `scripts/
  doctor_smoke/` runs `agentflow doctor --profile production` in
  a clean `ubuntu:24.04` Apple container; checked-in JSON
  fixture pins the expected first-run shape.
- **Production deployment dress-rehearsal** (P10.0.1): `scripts/
  production_dress_rehearsal/` walks all 6 steps + 2 in-container
  acceptance gates in a single container; pins doctor exit 0
  + status `ok` on properly-provisioned state.
- **GitHub Release workflow** (P10.0.4): `.github/workflows/
  release.yml` fires on `v*` tag push; builds 4-target CLI
  matrix (linux x86_64 / arm64, macOS Intel / Apple Silicon) +
  multi-arch `agentflow-server` GHCR image via buildx + GitHub
  Release with `SHA256SUMS.txt`.
- **Workspace metadata centralisation** (P10.0.2 + FU1): every
  workspace-internal `[dependencies]` now carries `version =
  "X.Y"`; shared `[workspace.package]` block in root Cargo.toml
  pins repo / homepage / license / authors. Pre-GA cosmetics
  resolved.

### Web UI alpha shell

- Run console + Harness session view + trace replay event timeline
  + preference UI wiring (P10.17.2) + server-side `?filter=` for
  long runs (P10.17.3) + Playwright e2e nightly (P10.17.4).
  Debugger-only positioning committed per P10.17.1 — operator
  dashboards stay in Grafana.

## Breaking Changes

This RC introduces no wire-breaking changes vs. v0.2.0 — every API
extension landed additively under the stability tiers documented in
`docs/STABILITY.md`. Operator-facing schema changes you may have to
adapt to:

- **Workspace-internal `path =` deps now require `version = "X.Y"`**
  (P10.0.2). Downstream forks that imported these crates by path
  without a `version` field will fail `cargo publish`; add the
  matching MAJOR.MINOR.
- **`LLMConfig::validate()` is no longer strict on missing keys**
  (P10.3.1). Callers that previously relied on fail-close behaviour
  for missing-key detection must switch to the new
  `validate_strict()` (the production-doctor path already migrated;
  custom callers should audit).
- **`security.os_sandbox` is now overridable per-tool** (P10.4.1) —
  additive (new `ToolConfig::os_sandbox: Option<bool>` field
  defaults to `None` = inherit manifest), but operators relying on
  the old "everything sandboxed if manifest says yes" invariant
  should review skills that ship explicit `[[tools]]` blocks.
- **Per-tool sandbox + admission gating** (P1.x closed). Skills
  authored before P1 may need to declare `tool_permission_allowlist`
  explicitly under `[security]` if they rely on tools that now
  require explicit opt-in (default tool allowlist is empty).
- **Workspace dependencies on `agentflow-rag` are pinned to
  `0.3.0-alpha`** (the rag crate's own pre-release). Downstream
  consumers must explicitly opt in to the pre-release version
  spec.

## Known Issues

Caught during the v1.0.0-rc.1 dress rehearsal (P10.0.1 / P10.0.5);
none block the RC but are tracked as future-PR follow-ups:

- **`agentflow serve --check` requires a real Postgres connection**
  despite a source comment claiming "non-binding readiness
  diagnostic which does not require Postgres". Either the comment
  is stale or the implementation drifted; the docker compose
  smoke path covers the production scenario regardless. Filed as
  a code-or-comment fix.
- **GHCR package visibility defaults to `private`** for newly
  published packages. After the first `release.yml` push, an admin
  must flip `ghcr.io/yuxuetr/agentflow-server` to public via
  GitHub package settings. Documented in
  `docs/RELEASE_CHECKLIST.md` §10.
- **`AgeEncryptedPreferenceStore` is not yet wired into the
  `agentflow memory` CLI** (P10.7.2 closure note). The wrapper +
  trait surface ships, but the CLI's prune / get / set
  subcommands still hard-code `SqlitePreferenceStore`. A
  `--encrypted --identity <path>` flag pair is a separate
  follow-up.
- **Anthropic's `/v1/models` lists only dated revisions for some
  model families** (e.g. `claude-haiku-4-5-20251001` is listed,
  the rolling alias `claude-haiku-4-5` is not). The live-test
  default uses the rolling alias; `cargo xtask refresh-live-models`
  recognises this pattern via `find_dated_revision` and reports
  `ok (via dated revision …)`. Not a release blocker but worth
  flagging for operators reading the refresh output.
- **`scripts/doctor_smoke/`'s expected-exit-2 outcome** on a
  fresh VM is documented behaviour, not a bug. Production-profile
  doctor on a zero-state Ubuntu reports `status: fail` because
  default `~/.agentflow/*` dirs don't exist — operators must
  pre-provision them before `agentflow serve`. The
  `production_dress_rehearsal` script walks the full setup +
  shows doctor exit 0 after the dirs exist.
