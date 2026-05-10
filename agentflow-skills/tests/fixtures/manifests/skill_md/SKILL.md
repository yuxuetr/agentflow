---
name: compat-skill
description: Compatibility test skill.
license: MIT
compatibility: AgentFlow 1.x
allowed-tools: file http
future-frontmatter-field: ignored
metadata:
  version: 2.1.0
  language: en
  future_metadata_key: preserved
mcp_servers:
  - name: demo
    command: python3
    args: ["server.py"]
    future_server_field: ignored
security:
  mcp_server_allowlist: ["demo"]
  mcp_command_allowlist: ["python3"]
  mcp_default_timeout_secs: 12
  future_security_field: ignored
---

# Compat Skill

Use this skill to verify SKILL.md compatibility parsing.
