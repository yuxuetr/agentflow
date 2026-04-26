# Skill Format Standard

AgentFlow supports two skill manifest formats:

- `SKILL.md`: recommended standard entrypoint. Use YAML frontmatter for metadata and Markdown for instructions.
- `skill.toml`: compatibility and structured runtime manifest. Use it when an existing skill already depends on TOML-only fields or when a deployment needs an explicit override.

When both files exist in the same skill directory, `skill.toml` is the active manifest. `SKILL.md` remains the portable, human-readable entrypoint, but the TOML file wins to preserve existing AgentFlow behavior.

## SKILL.md

Minimum format:

```markdown
---
name: code-reviewer
description: Review code for bugs, security issues, and maintainability.
allowed-tools: shell file script
metadata:
  version: "1.0.0"
---

# Code Reviewer

Read the referenced files, inspect risks, and report findings first.
```

AgentFlow extensions are declared directly in frontmatter:

```yaml
mcp_servers:
  - name: filesystem
    command: npx
    args:
      - -y
      - "@modelcontextprotocol/server-filesystem"
      - /tmp
    timeout_secs: 30
    env:
      LOG_LEVEL: info
```

The older `metadata.mcp_servers` JSON string form is still accepted for compatibility, but new skills should use structured `mcp_servers` frontmatter.

## skill.toml

`skill.toml` maps directly to `SkillManifest` and remains useful for structured fields such as model, memory, knowledge files, sandbox constraints, and script parameter schemas.

```toml
[skill]
name = "rust-expert"
version = "1.0.0"
description = "Rust review and implementation skill"

[persona]
role = "You are a senior Rust engineer."

[[mcp_servers]]
name = "local"
command = "python3"
args = ["server.py"]
```

## Naming

MCP tools are registered into the local tool registry as:

```text
mcp_<server_name>_<tool_name>
```

Non-alphanumeric characters are normalized to underscores and names are lowercased. Duplicate public tool names are rejected during skill build.
