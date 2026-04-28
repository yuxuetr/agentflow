# Secret Management

AgentFlow keeps provider credentials out of model and workflow configuration by default. Config files store environment variable names, while actual values live in process environment variables or `~/.agentflow/.env`.

## Current Loading Paths

- `agentflow config init` writes `~/.agentflow/models.yml` and a template `~/.agentflow/.env`.
- `AgentFlow::init()` loads `~/.agentflow/.env` first, then prefers `~/.agentflow/models.yml`, and falls back to built-in model defaults.
- LLM provider configs use `api_key_env` fields such as `OPENAI_API_KEY` and `STEPFUN_API_KEY`.
- Direct CLI audio/image commands currently read `STEPFUN_API_KEY` or `STEP_API_KEY` from the process environment.
- Skill MCP servers may receive environment variables from skill configuration; validation and CLI output must treat tool params and env-like values as sensitive.

## Storage Boundary

The current supported local secret store is the host environment plus `~/.agentflow/.env`. AgentFlow does not write plaintext API key values into `models.yml`.

Local encryption and external secret manager integration should be added behind a resolver boundary:

- `env:NAME` reads from the process environment or loaded `.env`.
- `file:/path` reads from a local secret file with caller-controlled permissions.
- `keychain:NAME`, `vault:PATH`, or cloud-specific resolvers can be added without changing model config schema.

Until that resolver exists, users should prefer OS-level secret managers, CI secret injection, Kubernetes Secrets, or `.env` files outside source control.

## Display Policy

- `agentflow config show` renders config through the shared redaction layer.
- Environment variable names such as `OPENAI_API_KEY` remain visible because they are identifiers, not credential values.
- Credential values in fields such as `api_key`, `token`, `authorization`, `password`, and tool params are replaced with `[REDACTED]`.
- Trace replay, trace TUI, workflow output, skill run/chat output, and tool params should default to redacted output.

## Validation Policy

`agentflow config validate` checks configuration shape and reports missing env variable names. It does not print current secret values.
