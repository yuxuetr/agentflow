# MCP Skills

MCP Skills let a Skill expose tools from one or more Model Context Protocol servers through AgentFlow's normal `ToolRegistry`. Agents then call those tools with the same ReAct loop and trace flow used for built-in, script, and workflow tools.

Use this guide when you want to package an external MCP server with a reusable Skill. For the broader Skill guide, see [SKILLS.md](SKILLS.md). For the lower-level architecture notes, see [MCP_SKILLS_INTEGRATION.md](MCP_SKILLS_INTEGRATION.md).

## Execution Path

At runtime, the path is:

```text
SKILL.md / skill.toml
  -> SkillLoader
  -> SkillBuilder
  -> McpClientPool
  -> MCP tools/list
  -> McpToolAdapter
  -> ToolRegistry
  -> ReActAgent tool call
  -> MCP tools/call
```

AgentFlow discovers MCP tools while building the skill registry. Each discovered remote tool is wrapped as a local `Tool`, preserving the remote description and input schema for prompts and CLI inspection.

## Minimal SKILL.md

```markdown
---
name: mcp-basic
description: Demonstrate a Skill that exposes tools from a local MCP server.
metadata:
  version: "1.0.0"
mcp_servers:
  - name: local-demo
    command: python3
    args:
      - ./server.py
    timeout_secs: 30
---

# MCP Basic

Use the local MCP demo tools when the user asks to echo text or inspect the demo server status.
```

`mcp_servers` fields:

- `name`: local server identifier used in public tool names.
- `command`: executable used to start the MCP server.
- `args`: command arguments. Optional.
- `env`: environment variables passed to the server. Optional.
- `timeout_secs`: timeout for connect, discovery, and tool calls. Defaults to 30 seconds.
- `max_concurrent_calls`: per-server MCP tool call admission limit. Defaults to 4.

Relative command parts such as `./server.py` are resolved from the skill directory.

Governance fields:

```toml
[security]
mcp_server_allowlist = ["local-demo"]
mcp_command_allowlist = ["python3"]
mcp_env_allowlist = ["MCP_TOKEN"]
mcp_default_timeout_secs = 30
mcp_max_concurrent_calls = 4
mcp_max_servers = 4
```

Empty `mcp_server_allowlist` allows all declared server names. `mcp_command_allowlist` defaults to `python`, `python3`, `node`, `npx`, and `uvx`. Environment variables are denied unless their keys are listed in `mcp_env_allowlist`.

## skill.toml Equivalent

Use `skill.toml` when you need a structured runtime override:

```toml
[skill]
name = "mcp-basic"
version = "1.0.0"
description = "Expose local MCP demo tools"

[persona]
role = "Use MCP tools when the user asks to echo text or inspect status."

[[mcp_servers]]
name = "local-demo"
command = "python3"
args = ["./server.py"]
timeout_secs = 30
```

When `SKILL.md` and `skill.toml` both exist, AgentFlow loads `skill.toml`.

## Tool Naming

Discovered MCP tools are registered as:

```text
mcp_<server_name>_<tool_name>
```

Names are lowercased, non-alphanumeric characters become underscores, and leading or trailing underscores are trimmed. Empty sanitized parts become `tool`.

Examples:

| MCP server | MCP tool | AgentFlow tool |
| --- | --- | --- |
| `local-demo` | `echo` | `mcp_local_demo_echo` |
| `github-server` | `search/repositories` | `mcp_github_server_search_repositories` |
| `!!!` | `???` | `mcp_tool_tool` |

Duplicate public names fail registry construction so an agent cannot call an ambiguous tool.

## Discovery And Schemas

During `SkillBuilder::build_registry`, AgentFlow starts each configured MCP server and calls `tools/list`. For each returned tool:

- MCP `description` becomes the local tool description.
- MCP `inputSchema` becomes the local tool parameter schema.
- The original remote tool is called by its MCP tool name, while the agent sees the public `mcp_<server>_<tool>` name.

Check the discovered tools:

```bash
cargo run -p agentflow-cli -- skill list-tools agentflow-skills/examples/skills/mcp-basic
```

Expected output includes:

```text
mcp_local_demo_echo
mcp_local_demo_status
text (string): Text to echo.
```

## Validation

Validate the manifest and MCP discovery path before running an agent:

```bash
cargo run -p agentflow-cli -- skill validate agentflow-skills/examples/skills/mcp-basic
```

Validation checks:

- The Skill manifest can be loaded.
- Required MCP server fields are present.
- MCP server processes can be started.
- `tools/list` succeeds.
- Discovered MCP tools can be registered without public name conflicts.

Validation prints the number of discovered MCP tools.

## Running A Skill

One-shot run:

```bash
cargo run -p agentflow-cli -- skill run agentflow-skills/examples/skills/mcp-basic \
  --message "echo hello through MCP" \
  --trace
```

Interactive run:

```bash
cargo run -p agentflow-cli -- skill chat agentflow-skills/examples/skills/mcp-basic
```

`--trace` on `skill run` prints the structured AgentRuntime trace, including tool call steps such as `mcp_local_demo_echo`.

## MCP Result Mapping

MCP `tools/call` results are converted into AgentFlow `ToolOutput` values:

- Text content is flattened into compatible string output.
- Typed content parts are preserved for tools that return text, image, or resource content.
- MCP error results become `ToolOutput::error`.
- Transport, timeout, and protocol failures become tool execution errors.

This keeps existing agents compatible with simple string outputs while preserving richer MCP content for callers that inspect structured parts.

## Timeouts And Shutdown

Each MCP server uses `timeout_secs` for:

- Initial connect.
- Tool discovery.
- Individual tool calls.

On timeout, AgentFlow disconnects the MCP client and clears the pool slot so later calls can reconnect cleanly. The MCP client pool also exposes explicit disconnect behavior used during validation failures and duplicate-name cleanup.

## Error Messages

CLI errors include the Skill context and MCP server details. Typical failures:

- Missing command: the configured executable does not exist or cannot be spawned.
- Discovery failure: the server starts but does not answer `tools/list`.
- Duplicate tool name: two discovered tools normalize to the same public name.
- Tool call failure: `tools/call` returns an MCP error or times out.

For broken MCP configs, `skill validate` reports the server name and command so the failure can be fixed without inspecting traces first.

## Security Notes

Treat MCP servers as executable dependencies:

- Prefer pinning server packages or using local scripts checked into the skill.
- Keep server commands and arguments explicit.
- Pass secrets through `env` only when the MCP server needs them, and list the keys in `mcp_env_allowlist`.
- Use short `timeout_secs` for tools that call external systems. Values are clamped to 1-120 seconds.
- Review audit logs for `mcp_server_config_audit`; they include server names, command names, arg counts, env keys, timeout, and concurrency limits.
- Validate a Skill before allowing an agent loop to use it.

MCP server permissions are controlled by the server implementation itself. AgentFlow wraps discovered tools but does not sandbox arbitrary work performed inside an external MCP process.

## Current Boundaries

Current MCP Skills support covers:

- `SKILL.md` and `skill.toml` `mcp_servers`.
- Stdio MCP server startup.
- Tool discovery through `tools/list`.
- Registration into `ToolRegistry`.
- Tool calls through `tools/call`.
- Description and input schema propagation.
- Tool source metadata set to `mcp`.
- Original MCP server and tool names preserved in tool metadata.
- Text, image, resource, and error content conversion.
- CLI validate/list-tools/run/chat coverage.

Known follow-up work:

- Add more user-facing examples for Skill-to-MCP agent runs.
- Expand docs for production MCP server packaging and secret handling.
