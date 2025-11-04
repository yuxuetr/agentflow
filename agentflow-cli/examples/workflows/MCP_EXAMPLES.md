# MCP Workflow Examples

This directory contains example workflows demonstrating Model Context Protocol (MCP) integration with AgentFlow.

## Prerequisites

1. **Install Node.js and npx** (required for MCP servers)
   ```bash
   # macOS
   brew install node

   # Ubuntu/Debian
   sudo apt install nodejs npm
   ```

2. **Install MCP filesystem server** (for these examples)
   ```bash
   npm install -g @modelcontextprotocol/server-filesystem
   ```

3. **Set API keys** (for LLM integration examples)
   ```bash
   export OPENAI_API_KEY=sk-...
   ```

## Examples Overview

### 1. `mcp_simple.yml` - Minimal MCP Integration

**Purpose**: Demonstrates the simplest possible MCP workflow with a single node.

**What it does**:
- Connects to MCP filesystem server
- Lists directory contents of `/tmp`

**Run**:
```bash
agentflow workflow run examples/workflows/mcp_simple.yml
```

**Expected output**: JSON list of files in `/tmp` directory.

---

### 2. `mcp_filesystem_example.yml` - Sequential MCP Operations

**Purpose**: Shows sequential MCP operations and integration with LLM.

**What it does**:
1. Lists directory contents via MCP
2. Reads a specific file via MCP
3. Uses LLM to summarize the file content

**Prerequisites**:
```bash
# Create a test file
echo "AgentFlow is a Rust-based workflow orchestration platform." > /tmp/test.txt
```

**Run**:
```bash
agentflow workflow run examples/workflows/mcp_filesystem_example.yml
```

**Expected output**:
- Directory listing
- File content
- LLM-generated summary

---

### 3. `mcp_code_analyzer.yml` - Advanced Hybrid Workflow

**Purpose**: Demonstrates a production-ready workflow combining MCP + LLM + Templates.

**What it does**:
1. Lists Rust files via MCP
2. Reads source code via MCP
3. Analyzes code quality with GPT-4
4. Suggests improvements with GPT-4
5. Formats report using template
6. Saves report via MCP

**Prerequisites**:
```bash
# Create a sample Rust file
cat > /tmp/example.rs << 'EOF'
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    let result = add(5, 3);
    println!("Result: {}", result);
}
EOF
```

**Run**:
```bash
agentflow workflow run examples/workflows/mcp_code_analyzer.yml
```

**Expected output**:
- Detailed code analysis
- Improvement suggestions
- Formatted markdown report saved to `/tmp/code_analysis_report.md`

---

## CLI Commands

AgentFlow also provides direct CLI commands for MCP operations:

### List Available Tools

```bash
agentflow mcp list-tools npx -y @modelcontextprotocol/server-filesystem /tmp
```

**Output**: Lists all tools provided by the MCP server with parameters.

### Call a Tool

```bash
# List directory
agentflow mcp call-tool \
  npx -y @modelcontextprotocol/server-filesystem /tmp \
  --tool list_directory \
  --params '{"path": "/tmp"}'

# Read a file
agentflow mcp call-tool \
  npx -y @modelcontextprotocol/server-filesystem /tmp \
  --tool read_file \
  --params '{"path": "/tmp/test.txt"}' \
  --output /tmp/mcp_result.json
```

### List Resources

```bash
agentflow mcp list-resources npx -y @modelcontextprotocol/server-filesystem /tmp
```

**Output**: Lists all resources provided by the MCP server.

---

## MCP Node Configuration Reference

### Basic Configuration

```yaml
nodes:
  - id: my_mcp_node
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
      tool_name: list_directory
      tool_params:
        path: "/tmp"
```

### Advanced Configuration

```yaml
nodes:
  - id: my_mcp_node
    type: mcp
    dependencies: ["previous_node"]  # Optional
    input_mapping:                    # Optional: dynamic parameters
      file_path: "{{ nodes.previous_node.outputs.path }}"
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
      tool_name: read_file
      tool_params:
        path: "{{ file_path }}"      # Can use templated values
      timeout_ms: 30000               # Optional: default 30000
      max_retries: 3                  # Optional: default 3
    run_if: "{{ condition }}"         # Optional: conditional execution
```

### Configuration Options

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `server_command` | Array[String] | Yes | - | Command to start MCP server |
| `tool_name` | String | Yes | - | Name of the tool to call |
| `tool_params` | Object | No | `{}` | Parameters to pass to the tool |
| `timeout_ms` | Number | No | 30000 | Timeout in milliseconds |
| `max_retries` | Number | No | 3 | Maximum retry attempts |

---

## Available MCP Servers

### Official MCP Servers

1. **Filesystem Server** (`@modelcontextprotocol/server-filesystem`)
   - Tools: `list_directory`, `read_file`, `write_file`, `create_directory`, `move_file`, `search_files`
   - Use case: File operations

2. **Git Server** (`@modelcontextprotocol/server-git`)
   - Tools: `git_status`, `git_diff`, `git_commit`, `git_log`
   - Use case: Git operations

3. **GitHub Server** (`@modelcontextprotocol/server-github`)
   - Tools: `create_issue`, `list_repositories`, `create_pull_request`
   - Use case: GitHub API integration

4. **PostgreSQL Server** (`@modelcontextprotocol/server-postgres`)
   - Tools: `query`, `list_tables`, `describe_table`
   - Use case: Database operations

5. **Everything Server** (`@modelcontextprotocol/server-everything`)
   - A test server with multiple tools for demonstration

### Community Servers

Visit [MCP Servers Registry](https://github.com/modelcontextprotocol/servers) for more.

---

## Troubleshooting

### Error: "Failed to connect to MCP server"

**Cause**: MCP server not installed or command incorrect.

**Solution**:
```bash
# Install the server
npm install -g @modelcontextprotocol/server-filesystem

# Verify installation
npx @modelcontextprotocol/server-filesystem --version
```

### Error: "Tool call failed"

**Cause**: Incorrect tool name or parameters.

**Solution**: List available tools first:
```bash
agentflow mcp list-tools npx -y @modelcontextprotocol/server-filesystem /tmp
```

### Error: "Timeout exceeded"

**Cause**: Server took too long to respond.

**Solution**: Increase timeout in workflow:
```yaml
parameters:
  timeout_ms: 60000  # 60 seconds
```

---

## Best Practices

1. **Use specific tool parameters**: Always validate tool parameters before execution
2. **Handle errors gracefully**: Use `run_if` conditions to handle failures
3. **Set appropriate timeouts**: Balance between reliability and performance
4. **Cache MCP connections**: Reuse server connections when calling multiple tools
5. **Validate server responses**: Check output format before passing to next node
6. **Use templates for dynamic parameters**: Leverage Tera templates for flexible workflows

---

## Next Steps

1. Try modifying the examples to use different MCP servers
2. Create custom workflows combining MCP with other AgentFlow nodes
3. Explore the [MCP specification](https://modelcontextprotocol.io/docs) for advanced features
4. Build custom MCP servers for your specific use cases

---

**Documentation Version**: 1.0
**Last Updated**: 2025-01-04
**AgentFlow Version**: 0.2.0
