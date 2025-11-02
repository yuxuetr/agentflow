# Model Context Protocol (MCP) Integration Guide

## Overview

AgentFlow supports the Model Context Protocol (MCP), enabling workflows to interact with external tools and resources through standardized MCP servers. This guide covers both CLI commands for direct MCP interaction and workflow integration for automated processes.

## Table of Contents

1. [What is MCP?](#what-is-mcp)
2. [Prerequisites](#prerequisites)
3. [CLI Commands](#cli-commands)
4. [Workflow Integration](#workflow-integration)
5. [Common MCP Servers](#common-mcp-servers)
6. [Troubleshooting](#troubleshooting)
7. [Advanced Usage](#advanced-usage)

---

## What is MCP?

The Model Context Protocol (MCP) is an open protocol that standardizes how applications provide context to LLMs. MCP servers expose:

- **Tools**: Executable functions with defined inputs/outputs
- **Resources**: Static or dynamic data sources
- **Prompts**: Template-based prompt management

AgentFlow's MCP integration allows workflows to:
- Call tools from any MCP-compatible server
- Access resources dynamically during execution
- Combine MCP capabilities with LLM reasoning

---

## Prerequisites

### 1. Install MCP Servers

MCP servers are typically distributed as npm packages. Common examples:

```bash
# Filesystem server (read/write files)
npx -y @modelcontextprotocol/server-filesystem

# Database server (query databases)
npx -y @modelcontextprotocol/server-database

# Web search server (search the web)
npx -y @modelcontextprotocol/server-web-search
```

### 2. Verify AgentFlow MCP Support

Ensure you have AgentFlow built with MCP support:

```bash
cargo build --features mcp
```

Or if using a pre-built binary, MCP support is included by default in v0.3.0+.

---

## CLI Commands

AgentFlow provides three main CLI commands for MCP interaction:

### 1. `agentflow mcp list-tools`

**Purpose**: Discover available tools from an MCP server

**Syntax**:
```bash
agentflow mcp list-tools <server-command> [OPTIONS]
```

**Arguments**:
- `<server-command>`: Command to execute the MCP server (space-separated)

**Options**:
- `--timeout-ms <milliseconds>`: Request timeout (default: 30000)
- `--max-retries <count>`: Maximum retry attempts (default: 3)

**Example**:
```bash
# List tools from filesystem server
agentflow mcp list-tools npx -y @modelcontextprotocol/server-filesystem /tmp

# With custom timeout
agentflow mcp list-tools npx -y @modelcontextprotocol/server-database --timeout-ms 60000
```

**Output**:
```
🔌 Connecting to MCP server: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
✅ Connected to MCP server

Available Tools (4):

  • list_directory
    List files and directories in a given path
    Parameters:
      - path (string): Directory path to list

  • read_file
    Read the contents of a file
    Parameters:
      - path (string): File path to read

  • write_file
    Write content to a file
    Parameters:
      - path (string): File path to write
      - content (string): Content to write

  • delete_file
    Delete a file
    Parameters:
      - path (string): File path to delete

Total: 4 tools available
```

---

### 2. `agentflow mcp call-tool`

**Purpose**: Execute a specific tool on an MCP server

**Syntax**:
```bash
agentflow mcp call-tool <server-command> --tool <tool-name> [OPTIONS]
```

**Arguments**:
- `<server-command>`: Command to execute the MCP server

**Options**:
- `-t, --tool <name>`: Tool name to call (required)
- `-p, --params <json>`: Tool parameters as JSON string
- `--timeout-ms <milliseconds>`: Request timeout (default: 30000)
- `--max-retries <count>`: Maximum retry attempts (default: 3)
- `-o, --output <file>`: Save result to file

**Examples**:

```bash
# Read a file
agentflow mcp call-tool npx -y @modelcontextprotocol/server-filesystem /tmp \
  --tool read_file \
  --params '{"path": "/tmp/test.txt"}'

# List directory contents
agentflow mcp call-tool npx -y @modelcontextprotocol/server-filesystem /tmp \
  --tool list_directory \
  --params '{"path": "/tmp"}'

# Write to a file and save result
agentflow mcp call-tool npx -y @modelcontextprotocol/server-filesystem /tmp \
  --tool write_file \
  --params '{"path": "/tmp/output.txt", "content": "Hello from AgentFlow!"}' \
  --output result.json
```

**Output**:
```
🔌 Connecting to MCP server: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
✅ Connected to MCP server

🔧 Calling tool: read_file with params: {"path":"/tmp/test.txt"}
✅ Tool call completed

Result:

{
  "content": [
    {
      "type": "text",
      "text": "This is the content of test.txt"
    }
  ]
}
```

---

### 3. `agentflow mcp list-resources`

**Purpose**: Discover available resources from an MCP server

**Syntax**:
```bash
agentflow mcp list-resources <server-command> [OPTIONS]
```

**Arguments**:
- `<server-command>`: Command to execute the MCP server

**Options**:
- `--timeout-ms <milliseconds>`: Request timeout (default: 30000)
- `--max-retries <count>`: Maximum retry attempts (default: 3)

**Example**:
```bash
agentflow mcp list-resources npx -y @modelcontextprotocol/server-filesystem /tmp
```

**Output**:
```
🔌 Connecting to MCP server: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
✅ Connected to MCP server

Available Resources (2):

  • config
    Application configuration file
    URI: file:///tmp/config.json
    MIME Type: application/json

  • data
    Application data directory
    URI: file:///tmp/data/

Total: 2 resources available
```

---

## Workflow Integration

### Basic MCP Node

Use the `mcp` node type in workflows to call MCP tools:

```yaml
name: "Simple MCP Workflow"
description: "Call an MCP tool in a workflow"

nodes:
  - id: read_config
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
      tool_name: read_file
      tool_params:
        path: "/tmp/config.json"
      timeout_ms: 30000
      max_retries: 3
```

### MCP with Dynamic Parameters

Use template syntax for dynamic parameters:

```yaml
nodes:
  - id: read_user_file
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/home/user"]
      tool_name: read_file
      tool_params:
        path: "{{ file_path }}"  # Resolved from workflow inputs or previous nodes
```

### Chaining MCP with LLM

Combine MCP data retrieval with LLM processing:

```yaml
name: "MCP + LLM Pipeline"
description: "Read file with MCP, analyze with LLM"

nodes:
  # Step 1: Read file using MCP
  - id: read_data
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/data"]
      tool_name: read_file
      tool_params:
        path: "/data/report.txt"

  # Step 2: Analyze content with LLM
  - id: analyze_report
    type: llm
    dependencies: ["read_data"]
    parameters:
      model: "gpt-4"
      system: "You are a data analyst expert."
      prompt: "Analyze the following report and provide key insights:\n\n{{ nodes.read_data.outputs.output }}"
      temperature: 0.7
      max_tokens: 1000
```

### Multiple MCP Servers

Use different MCP servers in the same workflow:

```yaml
nodes:
  # Read from filesystem
  - id: read_local_file
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
      tool_name: read_file
      tool_params:
        path: "/tmp/input.txt"

  # Query database
  - id: query_database
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-database", "postgresql://localhost/mydb"]
      tool_name: execute_query
      tool_params:
        query: "SELECT * FROM users WHERE active = true"

  # Combine results with LLM
  - id: generate_report
    type: llm
    dependencies: ["read_local_file", "query_database"]
    parameters:
      model: "gpt-4"
      prompt: "Generate a report combining file data and database records..."
```

### Error Handling

Configure retry behavior for resilient workflows:

```yaml
nodes:
  - id: retry_example
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-web-search"]
      tool_name: search
      tool_params:
        query: "latest news"
      timeout_ms: 60000    # 60 second timeout
      max_retries: 5       # Retry up to 5 times on transient errors
```

---

## Common MCP Servers

### 1. Filesystem Server

**Install**: `npx -y @modelcontextprotocol/server-filesystem`

**Tools**:
- `list_directory`: List files in a directory
- `read_file`: Read file contents
- `write_file`: Write to a file
- `delete_file`: Delete a file
- `move_file`: Move/rename a file
- `create_directory`: Create a directory

**Example**:
```bash
agentflow mcp list-tools npx -y @modelcontextprotocol/server-filesystem /home/user/documents
```

### 2. Database Server

**Install**: `npx -y @modelcontextprotocol/server-database`

**Tools**:
- `execute_query`: Run SQL queries
- `list_tables`: List database tables
- `describe_table`: Get table schema

**Example**:
```bash
agentflow mcp call-tool npx -y @modelcontextprotocol/server-database postgresql://localhost/mydb \
  --tool execute_query \
  --params '{"query": "SELECT COUNT(*) FROM users"}'
```

### 3. Web Search Server

**Install**: `npx -y @modelcontextprotocol/server-web-search`

**Tools**:
- `search`: Search the web
- `get_page`: Fetch webpage content

**Example**:
```bash
agentflow mcp call-tool npx -y @modelcontextprotocol/server-web-search \
  --tool search \
  --params '{"query": "Rust async programming", "max_results": 5}'
```

### 4. GitHub Server

**Install**: `npx -y @modelcontextprotocol/server-github`

**Tools**:
- `list_repos`: List repositories
- `get_repo`: Get repository details
- `list_issues`: List issues
- `create_issue`: Create a new issue

**Example**:
```bash
agentflow mcp call-tool npx -y @modelcontextprotocol/server-github \
  --tool list_repos \
  --params '{"owner": "anthropics", "per_page": 10}'
```

---

## Troubleshooting

### Connection Timeouts

**Problem**: `Failed to connect to MCP server: Timeout`

**Solutions**:
1. Increase timeout: `--timeout-ms 60000`
2. Check server command is correct
3. Verify server is accessible and responsive
4. Check network connectivity

### Tool Not Found

**Problem**: `Failed to call tool 'xyz': Tool not found`

**Solutions**:
1. List available tools first: `agentflow mcp list-tools ...`
2. Check tool name spelling (case-sensitive)
3. Verify server supports the tool
4. Update MCP server to latest version

### Parameter Validation Errors

**Problem**: `Invalid parameters: missing required field 'path'`

**Solutions**:
1. Check tool schema: `agentflow mcp list-tools ...`
2. Verify JSON parameter format
3. Include all required parameters
4. Check parameter types match schema

### Server Startup Failures

**Problem**: `Failed to start MCP server: spawn ENOENT`

**Solutions**:
1. Ensure npx is installed: `npm install -g npx`
2. Verify MCP server package name
3. Check npm registry access
4. Try running server command directly

---

## Advanced Usage

### Custom MCP Server Commands

Use any command to launch an MCP server:

```bash
# Python-based MCP server
agentflow mcp list-tools python -m my_mcp_server

# Go-based MCP server
agentflow mcp list-tools ./my-mcp-server --port 8080

# Docker-based MCP server
agentflow mcp list-tools docker run --rm mcp-server:latest
```

### Workflow with Conditional MCP Calls

```yaml
nodes:
  # Check if file exists
  - id: check_file
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/data"]
      tool_name: list_directory
      tool_params:
        path: "/data"

  # Conditional: only read if file exists
  - id: read_if_exists
    type: conditional
    dependencies: ["check_file"]
    parameters:
      condition: "{{ 'important.txt' in nodes.check_file.outputs.output }}"
      true_value:
        type: mcp
        parameters:
          server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/data"]
          tool_name: read_file
          tool_params:
            path: "/data/important.txt"
      false_value:
        type: llm
        parameters:
          model: "gpt-4"
          prompt: "File not found. Generate default content."
```

### Parallel MCP Calls

Execute multiple MCP calls concurrently:

```yaml
nodes:
  # These run in parallel (no dependencies)
  - id: read_file1
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/data"]
      tool_name: read_file
      tool_params:
        path: "/data/file1.txt"

  - id: read_file2
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/data"]
      tool_name: read_file
      tool_params:
        path: "/data/file2.txt"

  - id: read_file3
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/data"]
      tool_name: read_file
      tool_params:
        path: "/data/file3.txt"

  # Combine results after all complete
  - id: combine_results
    type: llm
    dependencies: ["read_file1", "read_file2", "read_file3"]
    parameters:
      model: "gpt-4"
      prompt: "Summarize these three files..."
```

---

## Best Practices

### 1. Use Appropriate Timeouts

- **File operations**: 10-30 seconds
- **Database queries**: 30-60 seconds
- **Web searches**: 30-90 seconds
- **Long-running tasks**: 2-5 minutes

### 2. Configure Retries Wisely

- **Network requests**: 3-5 retries
- **File operations**: 1-2 retries
- **Database queries**: 2-3 retries

### 3. Validate MCP Server Availability

Before running workflows, test MCP server connectivity:

```bash
# Quick validation
agentflow mcp list-tools npx -y @modelcontextprotocol/server-filesystem /tmp
```

### 4. Use Descriptive Node IDs

```yaml
# Good
- id: read_customer_data
- id: query_sales_records
- id: fetch_weather_api

# Avoid
- id: mcp1
- id: mcp2
- id: node3
```

### 5. Handle Errors Gracefully

Add error handling to workflows:

```yaml
nodes:
  - id: try_mcp_call
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/data"]
      tool_name: read_file
      tool_params:
        path: "/data/optional_file.txt"
      max_retries: 1  # Fail fast for optional operations

  - id: handle_error
    type: conditional
    dependencies: ["try_mcp_call"]
    parameters:
      condition: "{{ nodes.try_mcp_call.status == 'success' }}"
      true_value: "{{ nodes.try_mcp_call.outputs.output }}"
      false_value: "Using default data..."
```

---

## Performance Considerations

### Connection Pooling

For workflows with multiple MCP calls to the same server, consider:
- Reusing server instances when possible
- Grouping related calls in sequence
- Using workflow batching features

### Caching

Cache MCP results for repeated queries:

```yaml
nodes:
  - id: cached_query
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-database", "postgresql://localhost/mydb"]
      tool_name: execute_query
      tool_params:
        query: "SELECT * FROM config"
      cache_connection: true  # Future feature
```

---

## Security Considerations

### 1. Server Command Validation

Always validate MCP server commands:
- Use trusted server packages
- Avoid user-provided server commands
- Sanitize file paths and parameters

### 2. Parameter Sanitization

Sanitize MCP tool parameters:
```yaml
# Avoid direct user input
tool_params:
  path: "/data/{{ user_input }}"  # Risky!

# Use validated inputs
tool_params:
  path: "/data/{{ validated_filename }}"  # Better
```

### 3. Timeout Enforcement

Always set reasonable timeouts to prevent:
- Resource exhaustion
- Denial of service
- Workflow hangs

---

## Examples Repository

Find more MCP workflow examples at:
- `agentflow-cli/examples/workflows/mcp_simple.yml`
- `agentflow-cli/examples/workflows/mcp_filesystem_example.yml`

---

## Additional Resources

- **MCP Specification**: https://modelcontextprotocol.io
- **AgentFlow Documentation**: https://github.com/agentflow/agentflow
- **MCP Server Registry**: https://github.com/modelcontextprotocol/servers

---

## Getting Help

If you encounter issues with MCP integration:

1. Check server logs: `npx -y <server-package> --verbose`
2. Validate tool parameters: `agentflow mcp list-tools ...`
3. Test with minimal examples
4. Report bugs: https://github.com/agentflow/agentflow/issues

---

**Last Updated**: 2025-11-02
**AgentFlow Version**: 0.3.0+
**MCP Protocol Version**: 1.0
