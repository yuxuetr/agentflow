# Security Profiles

AgentFlow uses `AGENTFLOW_SECURITY_PROFILE` to select a coarse security
posture. The supported values are `dev`, `local`, and `production`.

If the variable is unset, AgentFlow uses `local`. This preserves the current
single-user CLI/server defaults while making the active posture visible to
operators through `agentflow doctor` and server startup logs.

## Profile Defaults

| Area | `dev` | `local` | `production` |
|------|-------|---------|--------------|
| Auth | API token optional. Unauthenticated loopback allowed. | API token optional. Unauthenticated loopback allowed. | API token required. Unauthenticated loopback disabled. |
| CORS | Permissive. | Permissive to preserve local behavior. | Explicit origin allow-list. Empty list means no browser origins are trusted until configured. |
| Request limits | 100 MiB body, 10 MiB workflow submit, 5 MiB Skill run. | 25 MiB body, 5 MiB workflow submit, 2 MiB Skill run. | 10 MiB body, 1 MiB workflow submit, 1 MiB Skill run. |
| Tool permissions | Filesystem read/write, process exec, network, MCP, workflow. | Filesystem read/write, process exec, network, MCP, workflow. | Filesystem read and workflow by default; no process exec or network by default. |
| Runtime capabilities | `fs.read`, `fs.write`, `exec`, `net`, `env`. | `fs.read`, `fs.write`, `exec`, `net`, `env`. | `fs.read` only by default. |
| OS sandbox | Optional; no-op backend allowed. | Optional; no-op backend allowed. | Required; no-op backend is not acceptable. |
| Plugin execution | Subprocess plugins allowed; sandbox opt-in. | Subprocess plugins allowed; sandbox opt-in. | Subprocess plugins disabled by default; OS sandbox required for future opt-in paths. |
| Marketplace installs | Remote installs allowed; signatures optional for fast iteration. | Remote installs allowed; signatures required; unsigned local fixtures allowed. | Remote installs allowed; signatures required; unsigned local fixtures rejected. |
| Run admission (V3.4) | 64 concurrent runs/tenant, 1000 submissions/min/tenant. | 32 concurrent runs/tenant, 300 submissions/min/tenant. | 10 concurrent runs/tenant, 60 submissions/min/tenant. |

## Current Wiring

P1.1 defines the shared model in `agentflow-tools` and wires profile
selection into:

- `agentflow-server`: reads `AGENTFLOW_SECURITY_PROFILE`, defaults to
  `local`, stores the selected defaults in `AppState`, and logs the active
  profile. When the selected profile requires auth, startup fails unless
  `AGENTFLOW_API_TOKEN` and/or `AGENTFLOW_API_TOKEN_TENANTS` is set to a
  non-empty value (either satisfies the requirement — see
  [DEPLOYMENT.md § Multi-tenant deployments](DEPLOYMENT.md#multi-tenant-deployments-bind-tokens-to-tenants-u11)
  for the token→tenant binding that closes cross-tenant header spoofing,
  U1.1). **V3.1:** the "Unauthenticated loopback allowed" row above is
  now actually enforced, not just documented — under every profile
  (including `dev`/`local`), a missing token only starts the gateway
  when the bind address is loopback (`127.0.0.1`/`::1`); a non-loopback
  bind (e.g. `0.0.0.0`, what the `PORT` env var always produces) with
  no token refuses to start regardless of the nominal profile. See
  [DEPLOYMENT.md § PORT and PaaS-style public binding](DEPLOYMENT.md#port-and-paas-style-public-binding-v31).
- `agentflow doctor`: reports the selected profile, effective defaults, and
  invalid profile warnings in text and JSON output.

Server startup also accepts explicit HTTP policy overrides:

- `AGENTFLOW_CORS_ALLOWED_ORIGINS`: comma-separated browser origins. In
  `production`, only these origins receive `Access-Control-Allow-Origin`.
- `AGENTFLOW_MAX_REQUEST_BODY_BYTES`: global documented request-body budget.
- `AGENTFLOW_MAX_WORKFLOW_SUBMIT_BYTES`: max JSON body for `POST /v1/runs`.
- `AGENTFLOW_MAX_SKILL_RUN_BYTES`: max JSON body for
  `POST /v1/skills/{name}:run`.
- `AGENTFLOW_MAX_CONCURRENT_RUNS_PER_TENANT` (V3.4): overrides
  `run_admission.max_concurrent_runs_per_tenant` — the number of
  in-process executor tasks a single tenant may have running at once
  via `POST /v1/runs`. A submission over the limit is rejected
  immediately (HTTP 429), not queued.
- `AGENTFLOW_RUN_SUBMIT_RATE_LIMIT_PER_MINUTE` (V3.4): overrides
  `run_admission.max_run_submissions_per_minute_per_tenant` — a
  fixed-window (60s) cap on how many runs a tenant may submit per
  minute. Both limits are per-tenant, not global; one noisy tenant
  can't starve another's quota.

The follow-up P1 tasks continue turning these defaults into enforcement:

- P1.4/P1.5 harden HTTP, file, and script tools.
- P1.6 exposes sandbox enforcement status in policy decisions.
- P1.8 applies plugin execution policy by profile.

## Compatibility Notes

`local` is intentionally the default profile. It keeps permissive CORS,
optional auth, optional OS sandboxing, subprocess plugins, and the existing
tool capability surface so existing local workflows continue to run. This
includes the Helm chart (`charts/agentflow/values.yaml`'s
`securityProfile` defaults to `local` for the same reason, U1.2) and
`docker-compose.yml` (`AGENTFLOW_SECURITY_PROFILE` unset) — see
[DEPLOYMENT.md § Security profile](DEPLOYMENT.md#security-profile-u12)
for what production deployments must set explicitly.

Use `production` only when the server or daemon may be reachable by other
users or hosts. Production mode now requires `AGENTFLOW_API_TOKEN` before the
server starts, but it is not yet a complete security boundary until the
remaining P1 enforcement tasks land.
