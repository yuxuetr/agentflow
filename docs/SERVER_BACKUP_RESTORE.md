# AgentFlow Server Backup & Restore

Status: stable as of `P2.7`.
Scope: covers `agentflow serve` deployments in the `local` and
`production` security profiles. Workflow-only single-binary
deployments back up the same state minus the Postgres bullet point.

This document is the operator playbook for backing up an AgentFlow
deployment and recovering from a fresh host. Use `agentflow doctor
--backup-check` to validate that a candidate host can actually receive
a restore before swinging traffic at it (see
[Validation](#validation) below).

## State surfaces

AgentFlow persists state across four surfaces. Each one is described
below in the order it must be backed up *and* restored — DB before
filesystem, because the DB rows reference paths under run_dir and
trace_dir.

### 1. Postgres (`agentflow-db`)

Required for the gateway, harness sessions, and the run console.
Eight authoritative tables, all owned by the schema bundled with
`agentflow-server`:

| Table | Owns |
| --- | --- |
| `runs` | DAG workflow runs (status, tenant, profile, dates) |
| `steps` | Per-node execution rows (status, retries, output ptr) |
| `events` | DAG workflow event log (`EventBroker` source of truth) |
| `artifacts` | Pointer rows for run outputs persisted under run_dir |
| `skill_installs` | Installed skill metadata + checksum |
| `mcp_sessions` | Live MCP session bookkeeping |
| `harness_sessions` | Harness Agent Mode session rows |
| `harness_session_events` | Harness event log (mirrors the SSE stream) |

Backup with `pg_dump` (or `agentflow backup` — see below). Restore
with `pg_restore`. Migrations are embedded via `sqlx::migrate!()`
and run on first server start, so a restored DB at the target
schema version Just Works; a restored DB at an *older* schema
version is upgraded by `agentflow serve` before it accepts
traffic.

### 2. Run artifacts (`AGENTFLOW_RUN_DIR`)

Default: `<home>/.agentflow/runs`. Each run produces one UUID-named
subdirectory containing the node-level state files referenced by
`artifacts` rows in Postgres. **Never restore filesystem before
Postgres** — orphaned artifact directories with no owning `runs` row
will be reaped by the next cleanup sweep (P2.2).

### 3. Trace storage (`AGENTFLOW_TRACE_DIR`)

Default: `<home>/.agentflow/traces`. JSONL / SQLite / Postgres backend
chosen at runtime via feature flags + env. The JSONL files are simple
log lines and back up via straight `tar`. SQLite + Postgres tracing
backends back up the same way as the main Postgres above.

### 4. Marketplace cache, skills, plugins

| Path | Default | Notes |
| --- | --- | --- |
| Marketplace cache | `<home>/.agentflow/marketplace/cache` | Read-only artifact cache; safe to rebuild on demand, but restoring it avoids re-downloading and re-verifying signatures. |
| Skills install dir | `<home>/.agentflow/skills` (override: `AGENTFLOW_SKILLS_DIR`) | Authoritative for installed skill manifests. Lose this and every skill needs to be reinstalled. |
| Plugins install dir | `<home>/.agentflow/plugins` (override: `AGENTFLOW_PLUGINS_DIR`) | Same shape as skills, but holds subprocess plugin binaries + manifests. |

## `agentflow backup` (P10.15.1)

`agentflow backup --output <path>` orchestrates the four state
surfaces above into a single bundle directory in one command:

```bash
agentflow backup --output /var/backups/agentflow/2026-05-20 \
  --database-url "$DATABASE_URL"
```

Output layout:

```text
<output>/
  manifest.json          # schema version, timestamps, per-artifact bytes
  db.dump                # pg_dump --format=custom of $DATABASE_URL
  run_dir.tar.gz         # tar -czf of $AGENTFLOW_RUN_DIR
  trace_dir.tar.gz       # tar -czf of $AGENTFLOW_TRACE_DIR
  marketplace_cache.tar.gz
  skills_dir.tar.gz
  plugins_dir.tar.gz
```

Key flags:

- `--output <PATH>` *(required)* — destination directory. The
  command creates it if missing; refuses to overwrite a
  non-empty directory unless `--force` is supplied.
- `--database-url <URL>` — Postgres connection string. Falls
  back to `$DATABASE_URL`. Only consulted when the `db` include
  is in the active set.
- `--include <NAME>` *(repeatable)* — restrict to one or more
  includes. Empty = all 6. Aliases accepted: `database` → `db`,
  `runs` → `run_dir`, `traces` → `trace_dir`, etc.
- `--dry-run` — print the plan + the `manifest.json` shape the
  command would emit, mutate nothing. Useful for production
  rehearsal before running with real credentials.
- `--force` — overwrite a non-empty `--output` directory.
- `--format text|json|json-envelope` — output format for the
  per-step report. `json-envelope` is the canonical
  `agentflow.cli/1` envelope; see `docs/CLI_JSON_OUTPUT.md`.

Failure handling: a missing source directory is `skipped` (not
a failure); a tool not on PATH (`pg_dump` / `tar`) is `failed`
and the exit code is `2`. Each step's row in the manifest /
report carries `status`, `bytes`, `duration_ms`, and a
`reason` field for skipped or failed steps. The bundle
manifest is written even on partial failure so a future
`agentflow restore` can diff "what we got" against "what we
wanted" instead of guessing.

The `manifest_version` field on the bundle (currently
`"agentflow.backup/1"`) is the wire-shape promise. Bumping it
is a breaking change for any future restore tooling that pins
to the prior shape.

Restore is **not** wrapped yet (tracked separately in v1.x — a
future `agentflow restore --input <path>` would consume the
same manifest); for now use the [Restore sequencing](#restore-sequencing)
steps below with `pg_restore` and `tar -xzf` directly.

## Restore sequencing

1. **Postgres first.** `pg_restore` into a fresh database, then point
   `agentflow serve --database-url` at it.
2. **Marketplace cache, skills, plugins.** These are independent of
   the DB and can restore in any order before the server starts.
3. **Trace storage.** Optional. Restore only if you need history for
   replay. Restoring trace files for runs whose rows are missing from
   `runs` is harmless but the rows will be unjoined.
4. **Run artifacts.** Restore last. Restoring run_dir before Postgres
   produces orphan trees that the next cleanup sweep deletes.
5. **Doctor smoke before traffic.** Run `agentflow doctor --profile
   production --backup-check` (and `--server <url>` once the gateway
   is bound) before swinging traffic. The check refuses to pass if any
   of the five backup-relevant dirs are non-writable, and refuses to
   pass if the server `<url>/health` probe doesn't return 2xx.

## Validation

`agentflow doctor --backup-check` is the deployment-time smoke. It
adds a `backup_check` section to the doctor report enumerating
writability probes for run_dir, trace_dir, marketplace_cache,
skills_dir, plugins_dir.

```text
$ agentflow doctor --profile production --backup-check
…
Backup check:
  run dir: ok (/var/agentflow/runs) [env]
  trace dir: ok (/var/agentflow/traces) [env]
  marketplace cache: ok (~/.agentflow/marketplace/cache) [default]
  skills dir: ok (~/.agentflow/skills) [default]
  plugins dir: ok (~/.agentflow/plugins) [default]
…
Status: ok
```

Production profile escalation rules:

| State | Production | Local / Dev |
| --- | --- | --- |
| Directory does not exist | Fail (exit 2) | Warning (exit 1) |
| Directory exists but is not writable | Fail (exit 2) | Fail (exit 2) |

Exit codes:

- `0` — every dir writable; backup readiness OK.
- `1` — one or more dirs missing (warn-level under local / dev).
- `2` — one or more dirs explicitly non-writable, or production profile
  with a missing dir.

## First-stable-release validation checklist

For the v1.0 release dress rehearsal (`P7.4`), run this checklist on
a freshly provisioned host before declaring the deployment ready:

- [ ] Postgres restored from latest backup; `agentflow serve --check`
  reports `db.reachable: true`.
- [ ] Marketplace cache restored; `agentflow doctor --backup-check`
  reports `marketplace cache: ok`.
- [ ] Skills install dir restored; `agentflow skill list` enumerates
  the expected skills.
- [ ] Plugins install dir restored (if any plugins are installed);
  `agentflow plugin list` enumerates them.
- [ ] Run artifacts restored (if required for the deployment's
  history retention policy).
- [ ] Trace storage restored (if required for replay).
- [ ] `agentflow doctor --profile production --backup-check --server
  http://127.0.0.1:8080` exits `0`.
- [ ] One representative run replays cleanly via `agentflow trace
  replay <run-id>`.

If any line fails, **do not swing traffic**. Fix the gap, re-run
`doctor --backup-check`, and only proceed once the report exits `0`.

## Related

- `docs/CHECKPOINT_RECOVERY.md` — workflow-level resume semantics.
- `docs/CURRENT_STATUS.md` — current state of the gateway and Web UI.
- `agentflow-server::cleanup` (P2.2) — the retention sweep that prunes
  orphaned run_dir subdirectories.
- `agentflow doctor --backup-check` (P2.7) — the deployment-time
  smoke this document refers to.
- `agentflow backup --output <path>` (P10.15.1) — the
  orchestrator that wraps `pg_dump` + `tar` into one command.
