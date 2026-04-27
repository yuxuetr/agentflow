# Tool Permission Model

AgentFlow exposes a stable permission model through `ToolMetadata`.
Every `ToolDefinition` now includes:

- `metadata.source`: `builtin`, `script`, `mcp`, or `workflow`
- `metadata.permissions.permissions`: normalized permission strings

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
