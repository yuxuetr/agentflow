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

## Current Wiring

P1.1 defines the shared model in `agentflow-tools` and wires profile
selection into:

- `agentflow-server`: reads `AGENTFLOW_SECURITY_PROFILE`, defaults to
  `local`, stores the selected defaults in `AppState`, and logs the active
  profile. When the selected profile requires auth, startup fails unless
  `AGENTFLOW_API_TOKEN` is set to a non-empty token.
- `agentflow doctor`: reports the selected profile, effective defaults, and
  invalid profile warnings in text and JSON output.

The follow-up P1 tasks continue turning these defaults into enforcement:

- P1.3 applies CORS and request body limits.
- P1.4/P1.5 harden HTTP, file, and script tools.
- P1.6 exposes sandbox enforcement status in policy decisions.
- P1.8 applies plugin execution policy by profile.

## Compatibility Notes

`local` is intentionally the default profile. It keeps permissive CORS,
optional auth, optional OS sandboxing, subprocess plugins, and the existing
tool capability surface so existing local workflows continue to run.

Use `production` only when the server or daemon may be reachable by other
users or hosts. Production mode now requires `AGENTFLOW_API_TOKEN` before the
server starts, but it is not yet a complete security boundary until the
remaining P1 enforcement tasks land.
