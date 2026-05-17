# AgentFlow v1.0.0-rc.1 Release Notes (DRAFT)

**Status**: Draft. The `## What's New`, `## Breaking Changes`, and
`## Known Issues` sections are filled in at tag time. The
`## Production Deployment Checklist` section below is the runbook
deliverable from `P7.4-FU4` and is stable independent of the tag cut.

**Linked rehearsal**: `docs/RELEASE_NOTES_DRESS_REHEARSAL.md`.

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

_To be filled in at tag time._

## Breaking Changes

_To be filled in at tag time._

## Known Issues

_To be filled in at tag time._
