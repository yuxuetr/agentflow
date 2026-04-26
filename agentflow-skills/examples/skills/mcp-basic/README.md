# mcp-basic Skill

Minimal Skill example that mounts a local stdio MCP server.

From the repository root:

```bash
cargo run -p agentflow-cli -- skill validate agentflow-skills/examples/skills/mcp-basic
cargo run -p agentflow-cli -- skill list-tools agentflow-skills/examples/skills/mcp-basic
```

Expected tools:

- `mcp_local_demo_echo`
- `mcp_local_demo_status`

The server is intentionally small and self-contained so it can be used as a fixture for CLI and documentation examples.
