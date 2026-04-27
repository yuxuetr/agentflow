# Skills

Skills package reusable agent capabilities for AgentFlow. A skill can define:

- Agent instructions and persona.
- Runtime limits such as model name, max iterations, and token budget.
- Built-in tools with sandbox constraints.
- Script tools from a local `scripts/` directory.
- MCP servers whose remote tools are discovered and registered at runtime.
- Knowledge files and `references/` documents injected into the agent context.
- Session or SQLite-backed memory configuration.

Use a Skill when behavior should be portable across CLI runs, agent runtimes, and future workflow integrations. Use a plain workflow when each step is deterministic and should be represented as a fixed DAG.

## Directory Layout

The recommended layout is:

```text
my-skill/
  SKILL.md
  references/
    policy.md
  scripts/
    helper.sh
```

`SKILL.md` is the recommended standard entrypoint. `skill.toml` is still supported for compatibility and for deployments that need an explicit structured override. When both files exist, `skill.toml` is loaded and overrides `SKILL.md`.

## SKILL.md

Minimal portable skill:

```markdown
---
name: code-reviewer
description: Review code for correctness, security, and maintainability.
allowed-tools: file shell
metadata:
  version: "1.0.0"
---

# Code Reviewer

Inspect the requested files and report findings first. Prioritize correctness,
security, data loss, and missing test coverage.
```

Important fields:

- `name`: lowercase identifier using letters, digits, and hyphens.
- `description`: short explanation of what the skill does and when to use it.
- `allowed-tools`: optional space-delimited built-in tool list. Supported values are `shell`, `file`, `http`, and `script`.
- `metadata.version`: optional version. Defaults to `1.0.0`.
- Markdown body: becomes the base persona/system instructions.

For the complete format reference, see [SKILL_FORMAT.md](SKILL_FORMAT.md).

## Built-In Tools

Skills can expose built-in local tools through `allowed-tools` in `SKILL.md` or `[[tools]]` in `skill.toml`.

```markdown
---
name: local-helper
description: Inspect local files and run approved commands.
allowed-tools: file shell script
---

# Local Helper

Use local files and scripts to answer implementation questions.
```

The current built-ins are:

- `file`: read, write, and list files under the active sandbox policy.
- `shell`: execute allowed commands.
- `http`: make HTTP requests subject to allowed domain policy.
- `script`: execute scripts from the skill's `scripts/` directory.

For tighter sandbox constraints, use `skill.toml`:

```toml
[skill]
name = "repo-helper"
version = "1.0.0"
description = "Inspect one repository safely"

[persona]
role = "Answer questions using the repository files and approved commands."

[[tools]]
name = "file"
allowed_paths = ["./src", "./docs"]

[[tools]]
name = "shell"
allowed_commands = ["cargo", "rg"]
max_exec_time_secs = 60
```

## Script Tools

Declare the `script` tool and place executable helpers in `scripts/`:

```text
my-skill/
  SKILL.md
  scripts/
    summarize.sh
```

When the skill is built, AgentFlow registers one `script` tool rooted at that directory. The script tool always validates wrapper parameters with JSON Schema before execution. By default it only accepts a plain `script` filename ending in `.py`, `.sh`, or `.js`, plus optional `args`; extra top-level parameters are rejected.

Script parameters can be further constrained with a JSON schema through `skill.toml`:

```toml
[[tools]]
name = "script"

[tools.parameters]
type = "object"
required = ["script"]

[tools.parameters.properties.script]
type = "string"

[tools.parameters.properties.args]
type = "array"
```

For sandboxing, script tools resolve and canonicalize the target path before execution. Symlinks or paths that escape `scripts/` are rejected. If a skill declares only the `script` tool and omits `allowed_commands`, AgentFlow allows only the known script interpreters: `python3`, `bash`, and `node`.

## MCP Tools

Skills can mount MCP servers declaratively. AgentFlow starts the configured server, calls `list_tools`, and registers each discovered remote tool in the local `ToolRegistry`.

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

Use the local MCP demo tools when the user asks to echo text or inspect status.
```

MCP tool names are exposed as:

```text
mcp_<server_name>_<tool_name>
```

Names are lowercased, non-alphanumeric characters are normalized to underscores, and duplicate public names fail validation. For example, server `local-demo` tool `echo` becomes `mcp_local_demo_echo`.

Relative MCP command parts such as `./server.py` are resolved from the skill directory. MCP connect, discovery, and tool calls use `timeout_secs`, defaulting to 30 seconds.

MCP startup is governed by `[security]` in `skill.toml` or matching frontmatter fields in `SKILL.md`. By default, AgentFlow allows only common stdio server launchers (`python`, `python3`, `node`, `npx`, `uvx`), limits a skill to 4 MCP servers, clamps MCP timeouts to 1-120 seconds, and admits at most 4 concurrent calls per server. If an MCP server forwards environment variables, list their names in `mcp_env_allowlist`; audit logs record command names and env keys, never env values.

See [MCP_SKILLS.md](MCP_SKILLS.md) for the MCP Skills usage guide and [MCP_SKILLS_INTEGRATION.md](MCP_SKILLS_INTEGRATION.md) for the deeper design notes.

## Knowledge And References

There are two ways to add local context:

- `references/`: standard Skill directory for Markdown or text files. AgentFlow loads `.md` and `.txt` files in deterministic order and injects them into the persona.
- `[[knowledge]]`: `skill.toml` entries for explicit files or glob patterns.

Example:

```toml
[[knowledge]]
path = "./knowledge/*.md"
description = "Project operating notes"
```

Today this context is injected directly into the agent prompt. Semantic retrieval and memory budget summarization are tracked separately in the runtime roadmap.

## Memory

Skills can configure memory in `skill.toml`:

```toml
[memory]
type = "sqlite"
db_path = "~/.agentflow/memory/repo-helper.db"
window_tokens = 8000
```

Supported values are:

- `session`: in-memory session window.
- `sqlite`: persistent SQLite-backed memory.
- `none`: currently maps to the default in-memory session behavior while disabling persistent storage.

`semantic` appears in the manifest type but is not enabled by the current validator. Treat semantic memory as upcoming runtime work, not as a stable Skill feature.

## CLI Workflow

Create a new skill scaffold:

```bash
cargo run -p agentflow-cli -- skill init ./my-skill \
  --description "Describe when this skill should be used."
```

The scaffold includes `SKILL.md`, `README.md`, `references/example.md`, `scripts/hello.py`, and `tests/smoke.sh`.

Inspect a shared registry index:

```bash
cargo run -p agentflow-cli -- skill index validate ./skills.index.toml
cargo run -p agentflow-cli -- skill index list ./skills.index.toml
cargo run -p agentflow-cli -- skill index resolve ./skills.index.toml sample-skill
```

`skills.index.toml` is a local, organization-owned catalog. Each entry pins a skill `version` and can optionally lock the manifest with `manifest_sha256`. Relative `path` values are resolved from the index file directory, so a repository can keep shared skills and the index side by side.

Validate a skill:

```bash
cargo run -p agentflow-cli -- skill validate agentflow-skills/examples/skills/mcp-basic
```

List all tools exposed by a skill:

```bash
cargo run -p agentflow-cli -- skill list-tools agentflow-skills/examples/skills/mcp-basic
```

Run the skill test gate:

```bash
cargo run -p agentflow-cli -- skill test agentflow-skills/examples/skills/mcp-basic
```

`skill test` runs manifest validation, tool discovery, and built-in minimal regressions. For the default scaffold it invokes `scripts/hello.py` through the script tool. Pass `--smoke` to also run `tests/smoke.sh` when present.

Run one message through a skill:

```bash
cargo run -p agentflow-cli -- skill run agentflow-skills/examples/skills/mcp-basic \
  --message "echo hello through MCP"
```

Start an interactive session:

```bash
cargo run -p agentflow-cli -- skill chat agentflow-skills/examples/skills/mcp-basic
```

`skill run` and `skill chat` build a `ReActAgent` from the manifest, attach the built tool registry, and execute through the agent runtime. Use `--trace` with `skill run` to print the structured runtime trace.

## Current Boundaries

The current Skill path covers packaging, validation, tool registry construction, MCP tool discovery, MCP tool execution, prompt-time references, and agent runtime execution.

Known follow-up work is tracked in `TODOs.md` and `RoadMap.md`, including:

- Agent runtime disable switch.
- Semantic memory query APIs and agent loop memory hooks.
- Memory budget and summarization strategy.
- More examples for Skill-to-MCP calls.
