# Tool Permission Model

AgentFlow exposes a stable permission model through `ToolMetadata`.
Every `ToolDefinition` now includes:

- `metadata.source`: `builtin`, `script`, `mcp`, or `workflow`
- `metadata.permissions.permissions`: normalized permission strings
- `metadata.idempotency`: `idempotent`, `non_idempotent`, or `unknown`

## Permissions

- `filesystem_read`: read local filesystem state
- `filesystem_write`: write or mutate local filesystem state
- `process_exec`: execute local commands or scripts
- `network`: make outbound network requests
- `mcp`: connect to or invoke MCP servers
- `workflow`: execute nested AgentFlow workflows

## Defaults By Source

- builtin `shell`: `process_exec`
- builtin `file`: `filesystem_read`, `filesystem_write`
- builtin `http`: `network`
- `script`: `process_exec`, `filesystem_read`
- `mcp`: `mcp`, `network`
- `workflow`: `workflow`

The permission model is inspectable today and is intended as the common
surface for future enforcement, audit logging, and policy configuration.

## Idempotency

`ToolMetadata.idempotency` is the static default. Tools whose replay safety
depends on inputs implement `Tool::idempotency(params)`:

- file `read` / `list`: `idempotent`
- file `write`: `non_idempotent`
- HTTP `GET`: `idempotent`
- HTTP `POST`: `non_idempotent`
- shell and script tools: `non_idempotent`
- MCP tools: `unknown` unless the description or schema declares a hint

Agent runtime traces copy known idempotency into `_agentflow.side_effect_class`.
`AgentNode` uses that durable metadata during checkpoint resume: idempotent
unresolved calls can be replayed, while non-idempotent or unknown unresolved
calls require manual recovery.
