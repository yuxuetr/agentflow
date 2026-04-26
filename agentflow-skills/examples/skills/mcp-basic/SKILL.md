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
---

# MCP Basic

Use the local MCP demo tools when the user asks to echo text or inspect the demo server status.

The MCP tools are registered in AgentFlow as:

- `mcp_local_demo_echo`
- `mcp_local_demo_status`
