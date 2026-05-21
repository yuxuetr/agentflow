# Production deployment dress rehearsal (P10.0.1)

End-to-end reproduction of the 6-step
[`docs/RELEASE_NOTES_v1.0.0-rc.1.md::Production Deployment Checklist`](../../docs/RELEASE_NOTES_v1.0.0-rc.1.md#production-deployment-checklist)
inside a fresh `ubuntu:24.04` container. Captures the canonical
`agentflow doctor --profile production` exit code (the explicit
deliverable from the TODO) plus the two server-side acceptance gates
that don't require an external Postgres.

Companion to `scripts/doctor_smoke/`:

* `doctor_smoke/`              — checks "doctor on a totally fresh box"
  (status: fail / exit 2, the documented first-run signal).
* `production_dress_rehearsal/` — checks "doctor on a properly
  configured production-profile box" (status: ok / exit 0, the
  go-to-prod gate).

## Usage

```sh
scripts/production_dress_rehearsal/run.sh
```

`PROD_REHEARSAL_RUNTIME=docker` switches to Docker (default is
Apple's `container` CLI). First run takes ~12-18 min (Rust toolchain
+ both `agentflow` and `agentflow-server` binaries); cache hits bring
re-runs down to ~10 s.

## Files

| Path                        | Purpose |
|-----------------------------|---------|
| `Containerfile`             | Two-stage build: rust:1-slim-bookworm builds both binaries → ubuntu:24.04 carries them + ca-certificates + openssl. |
| `inside_container.sh`       | The in-container script that walks all 6 steps + the 4 acceptance gates, records per-step outcomes, emits a JSON summary. |
| `run.sh`                    | Host-side driver: builds the image (if not cached) + pipes `inside_container.sh` to `bash -s` inside a fresh container + extracts the JSON summary. |
| `last-run.log`              | Full step-by-step transcript with `✓ / ✗` marks. **Not checked in** (matches the repo-wide `*.log` gitignore); regenerated locally on every `run.sh`. |
| `last-run.json`             | Structured per-step outcome summary parseable by `jq`. **Checked in** as the canonical "passing" reference. |
| `README.md`                 | This file. |

## What gets validated in-container

| # | Step (from the checklist) | Outcome |
|---|---------------------------|---------|
| 1 | `AGENTFLOW_SECURITY_PROFILE=production` | **PASS** — env exported, doctor reports `security.profile = "production"`. |
| 2 | Provision `AGENTFLOW_API_TOKEN` (≥ 32 chars CSPRNG) | **PASS** — 64-hex-char token from `openssl rand -hex 32`. |
| 3 | Pre-provision 5 storage directories | **PASS** — `/var/lib/agentflow/{runs,traces,marketplace-cache,skills,plugins}` created via `install -d -m 0750`; all 5 reachable + writable. |
| 4 | Wire `DATABASE_URL` | **PASS-NOTED** — env exported pointing at `db.invalid:5432`. The single-container rehearsal does not provision a Postgres sidecar; actual connectivity is validated host-side via docker compose (see "Host-side follow-ups" below). |
| 5 | `agentflow doctor --profile production --backup-check --format json` | **PASS** — exit code 0, `status: "ok"`. This is the canonical deliverable of the TODO. |
| 6 | docker compose smoke (`docker compose up -d`, hit `/health/{live,ready}`, open `/ui`) | **SKIP** — requires docker-in-docker; out of scope for the single-container rehearsal. |
| AG1 | doctor exits 0 | **PASS** (same evidence as step 5). |
| AG2 | `agentflow serve --check --security-profile production` | **SKIP-NEEDS-POSTGRES** — the check runs the config-resolution stage cleanly but then attempts a real DB connection. With `db.invalid` the connection fails. **Note**: this contradicts the source-code comment in `agentflow-cli/src/commands/serve.rs` ("non-binding readiness diagnostic which does not require Postgres"); the implementation has drifted. Filed for follow-up. |
| AG3 | `/health/ready` returns 200 | **SKIP** — requires running server + Postgres; host-side. |
| AG4 | Authenticated `POST /v1/runs` completes | **SKIP** — requires running server + Postgres; host-side. |

The script exits 0 when no step is `fail`. `skip` and
`pass-noted` outcomes don't count against the rehearsal — they're
documented limitations of the single-container shape, not bugs in
the binary.

## Host-side follow-ups (steps 6 / AG3 / AG4)

For full validation including a real Postgres + a running server,
use the reference `docker-compose.yml` at the repo root:

```sh
# 1. Generate a token + write the .env block
export AGENTFLOW_API_TOKEN="$(openssl rand -hex 32)"
export AGENTFLOW_SECURITY_PROFILE=production
# (edit docker-compose.yml to source AGENTFLOW_API_TOKEN from the host env,
#  or use --env-file with a 0600 .env file outside the repo)

# 2. Bring the stack up
docker compose up -d

# 3. Step 6: confirm both services healthy
docker compose ps                                # both → (healthy)
curl -fsS http://localhost:3000/health/live      # AG3
curl -fsS http://localhost:3000/health/ready     # AG3
open       http://localhost:3000/ui              # SPA shell renders

# 4. AG4: authenticated run submission
curl -fsS -X POST http://localhost:3000/v1/runs \
  -H "Authorization: Bearer ${AGENTFLOW_API_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"workflow":"name: noop\nnodes: []\n"}'
```

The host-side commands above run on a machine with Docker; they
intentionally aren't scripted here because:

1. Docker-in-Apple-container is painful (rootless + nested
   virtualisation overhead) and unreliable.
2. Step 6 is **explicitly optional** in the upstream checklist.
3. AG3/AG4 are post-deploy validation, not pre-deploy gates — the
   operator pairs them with their actual deployment target (k8s,
   systemd, ECS, etc.), which varies.

## Why a checked-in fixture instead of an assertion?

Same rationale as `scripts/doctor_smoke/README.md` § "Why a
checked-in fixture": the JSON shape includes strings that may
localise / timestamp / version-tag in future. The fixture is
documentation + diff target, not a strict oracle. The driver's
`jq` step-matrix table is the operator-facing surface.

## When to run

* Before cutting a release (paired with the doctor smoke).
* After PRs that touch `doctor`, `serve --check`, or the
  `AGENTFLOW_SECURITY_PROFILE=production` enforcement code path.
* After bumping the workspace's Rust toolchain pin.

Like the doctor smoke, this is **not** wired into `quality.yml` —
the 12-18 min build cost is wrong fit for per-PR gating. Manual
pre-release operator step.

## Filed follow-ups

* `AG2_SERVE_CHECK_NEEDS_DB` — the source comment claims `serve
  --check` is non-binding diagnostics, but the implementation
  attempts a DB connection. Either the comment or the
  implementation should be updated. Not a release blocker
  (the host-side compose flow exercises the same code path with
  a real DB), but a stale-comment item for a future PR.
