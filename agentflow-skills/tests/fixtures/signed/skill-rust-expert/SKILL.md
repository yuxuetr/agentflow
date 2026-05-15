---
name: rust-expert
description: Locally-signed test skill for P5.2 marketplace signature gating.
license: MIT
compatibility: AgentFlow 1.x
allowed-tools: file
metadata:
  version: 1.0.0
  language: en
mcp_servers: []
security:
  mcp_server_allowlist: []
  mcp_command_allowlist: []
  mcp_default_timeout_secs: 10
---

# Rust Expert (signed test fixture)

This is a minimal Skill package used by the P5.2 marketplace
signature fixture tests. It is not intended to be installed at
runtime — see `tests/marketplace_signed.rs` in `agentflow-skills`.
