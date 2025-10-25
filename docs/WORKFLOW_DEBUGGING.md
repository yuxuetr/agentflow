# Workflow Debugging Guide

**Status**: ✅ Implemented (v0.2.0)
**Module**: `agentflow-cli::commands::workflow::debug`

## Overview

AgentFlow provides comprehensive debugging tools to help you inspect, analyze, and validate workflows before execution. The `workflow debug` command offers multiple inspection modes to understand workflow structure, dependencies, and execution plans.

## Features

- 🌳 **DAG Visualization**: Text-based visualization of workflow graph
- ✅ **Validation**: Detect configuration errors and potential issues
- 📊 **Analysis**: Workflow metrics and complexity analysis
- 📅 **Execution Planning**: See how workflows will execute
- 🧪 **Dry Run**: Simulate workflow execution without running nodes
- 🔍 **Bottleneck Detection**: Identify potential performance issues

## Quick Start

```bash
# Show all debug information
agentflow workflow debug my_workflow.yml

# Specific analysis modes
agentflow workflow debug my_workflow.yml --visualize
agentflow workflow debug my_workflow.yml --validate
agentflow workflow debug my_workflow.yml --analyze
agentflow workflow debug my_workflow.yml --plan
agentflow workflow debug my_workflow.yml --dry-run

# Combine flags
agentflow workflow debug my_workflow.yml --visualize --plan --verbose
```

## Command Reference

### Basic Usage

```bash
agentflow workflow debug <workflow_file> [FLAGS]
```

### Flags

| Flag | Description |
|------|-------------|
| `--visualize` | Visualize the workflow DAG structure |
| `--validate` | Validate workflow configuration |
| `--analyze` | Analyze workflow metrics and complexity |
| `--plan` | Show execution plan with parallelism |
| `--dry-run` | Simulate workflow execution |
| `-v, --verbose` | Enable detailed output |

**Note**: If no flags are specified, all modes except `--dry-run` are shown.

## Debugging Modes

### 1. Workflow Validation

Validates workflow configuration and detects common issues:

```bash
agentflow workflow debug my_workflow.yml --validate
```

**Checks for**:
- Empty workflows (no nodes defined)
- Duplicate node IDs
- Invalid dependencies (non-existent nodes)
- Circular dependencies
- Unreachable nodes (warnings)
- Node type distribution

**Example Output**:
```
═══════════════════════════════════════════════════════════
📋 WORKFLOW VALIDATION
═══════════════════════════════════════════════════════════

Workflow: AI Research Assistant
Total nodes: 6

✅ No validation issues found

Node types summary:
  - llm: 3
  - while: 1
  - template: 1
  - markmap: 1
```

### 2. DAG Visualization

Displays workflow structure as a text-based tree:

```bash
agentflow workflow debug my_workflow.yml --visualize
```

**Shows**:
- Node hierarchy and dependencies
- Node types
- Parameter counts (with `--verbose`)
- Dependency relationships

**Example Output**:
```
═══════════════════════════════════════════════════════════
🌳 WORKFLOW VISUALIZATION
═══════════════════════════════════════════════════════════

Workflow: AI Research Assistant

├─ [while] search_loop
   └─ 6 parameter(s)
  └─ [llm] summarize_paper
     └─ 2 parameter(s)
    └─ [llm] detect_language
       └─ 1 parameter(s)
    └─ [template] final_summary_selector
       └─ 1 parameter(s)

Dependencies:
  summarize_paper ← ["search_loop"]
  detect_language ← ["summarize_paper"]
  final_summary_selector ← ["summarize_paper"]
```

### 3. Workflow Analysis

Analyzes workflow complexity and structure:

```bash
agentflow workflow debug my_workflow.yml --analyze
```

**Metrics Provided**:
- Total nodes count
- Dependency statistics (total, average, maximum)
- Workflow depth (execution levels)
- Bottleneck detection (nodes with many dependents)
- Parallelism opportunities
- Node type distribution

**Example Output**:
```
═══════════════════════════════════════════════════════════
📊 WORKFLOW ANALYSIS
═══════════════════════════════════════════════════════════

Workflow Metrics:
  Total nodes:        6
  Total dependencies: 5
  Average dependencies per node: 0.8
  Max dependencies on single node: 1
  Workflow depth:     4 levels
  Max dependents:     2

Node Type Distribution:
  - llm          :   3 ( 50.0%)
  - while        :   1 ( 16.7%)
  - template     :   1 ( 16.7%)
  - markmap      :   1 ( 16.7%)
```

**Bottleneck Detection**:
```
⚠️  Potential bottlenecks (nodes with many dependents):
  - 'data_fetch': 5 nodes depend on it
  - 'preprocessing': 4 nodes depend on it
```

### 4. Execution Plan

Shows how the workflow will execute with parallelism information:

```bash
agentflow workflow debug my_workflow.yml --plan
```

**Shows**:
- Execution levels (stages)
- Parallel execution opportunities
- Node dependencies (with `--verbose`)
- Maximum parallelism factor

**Example Output**:
```
═══════════════════════════════════════════════════════════
📅 EXECUTION PLAN
═══════════════════════════════════════════════════════════

Estimated Execution Plan:
(Nodes at the same level can execute in parallel)

Level 0 (1 node):
  ├─ [while] search_loop - (no dependencies)

Level 1 (1 node):
  ├─ [llm] summarize_paper - depends on: ["search_loop"]

Level 2 (2 nodes):
  ├─ [llm] detect_language - depends on: ["summarize_paper"]
  ├─ [template] final_summary_selector - depends on: ["summarize_paper"]

Level 3 (2 nodes):
  ├─ [llm] translate_summary - depends on: ["detect_language"]
  ├─ [markmap] create_mindmap - depends on: ["final_summary_selector"]

Total execution levels: 4
Maximum parallelism: 2
```

### 5. Dry Run

Simulates workflow execution without actually running nodes:

```bash
agentflow workflow debug my_workflow.yml --dry-run
```

**Shows**:
- Step-by-step execution simulation
- Node execution order
- Dependencies at each step
- Parameter configuration status

**Example Output**:
```
═══════════════════════════════════════════════════════════
🧪 DRY RUN
═══════════════════════════════════════════════════════════

Simulating workflow execution: AI Research Assistant

📍 Level 0 - Executing 1 node(s)
  ▶️  Node: search_loop
      Type: while
      Parameters: 6 configured
      ✓ Simulation passed

📍 Level 1 - Executing 1 node(s)
  ▶️  Node: summarize_paper
      Type: llm
      Dependencies: ["search_loop"]
      Parameters: 2 configured
      ✓ Simulation passed

...

✅ Dry run completed successfully
   Total levels: 4
   Total nodes: 6
```

## Use Cases

### 1. Pre-flight Validation

Before running a complex workflow, validate its configuration:

```bash
agentflow workflow debug production_pipeline.yml --validate
```

This catches:
- Typos in node IDs
- Missing dependencies
- Circular dependency loops
- Configuration errors

### 2. Optimize Parallelism

Analyze workflow to maximize parallel execution:

```bash
agentflow workflow debug data_pipeline.yml --analyze --plan
```

Look for:
- Nodes at the same level (can run in parallel)
- Bottlenecks (high dependent count)
- Opportunities to restructure for better parallelism

### 3. Understand Complex Workflows

Visualize large or unfamiliar workflows:

```bash
agentflow workflow debug complex_workflow.yml --visualize --verbose
```

Quickly understand:
- Overall workflow structure
- Node relationships
- Execution flow

### 4. Debug Workflow Issues

When a workflow fails or behaves unexpectedly:

```bash
agentflow workflow debug failing_workflow.yml --analyze --dry-run --verbose
```

Identify:
- Unreachable nodes
- Unexpected dependencies
- Configuration problems

## Common Patterns

### Quick Health Check

```bash
# Validate and show structure
agentflow workflow debug workflow.yml --validate --visualize
```

### Performance Analysis

```bash
# Analyze parallelism and bottlenecks
agentflow workflow debug workflow.yml --analyze --plan --verbose
```

### Pre-production Verification

```bash
# Comprehensive check before deployment
agentflow workflow debug workflow.yml
```

### Development Workflow

```bash
# Quick validation during development
agentflow workflow debug workflow.yml --validate --plan
```

## Validation Error Examples

### Circular Dependency

```
❌ Issues found: 1
  1. Circular dependency detected: node_a -> node_b -> node_a
```

**Solution**: Remove the circular dependency by restructuring the workflow.

### Missing Dependency

```
❌ Issues found: 1
  1. Node 'process_data' depends on non-existent node 'fetch_dat'
```

**Solution**: Fix the typo in the dependency reference (`fetch_dat` → `fetch_data`).

### Unreachable Node

```
⚠️  Warnings: 1
  1. Node 'cleanup' may be unreachable
```

**Solution**: Ensure the node has dependencies or is a root node.

## Best Practices

### 1. Validate Before Running

Always validate workflows before execution:

```bash
agentflow workflow debug workflow.yml --validate
```

### 2. Use Verbose Mode for Details

When debugging issues, use verbose mode:

```bash
agentflow workflow debug workflow.yml --verbose
```

### 3. Optimize for Parallelism

Review execution plans to identify parallelism opportunities:

```bash
agentflow workflow debug workflow.yml --plan --analyze
```

Look for bottlenecks and restructure if needed.

### 4. Monitor Complexity

For large workflows, check complexity metrics:

```bash
agentflow workflow debug large_workflow.yml --analyze
```

Consider splitting if:
- Depth > 10 levels
- Max dependents > 5
- Total nodes > 50

### 5. Document Workflow Structure

Generate visualizations for documentation:

```bash
agentflow workflow debug workflow.yml --visualize > docs/workflow_structure.txt
```

## Integration with CI/CD

### Validation in CI Pipeline

```yaml
# .github/workflows/validate.yml
name: Validate Workflows

on: [push, pull_request]

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Install AgentFlow
        run: cargo install agentflow-cli
      - name: Validate Workflows
        run: |
          for workflow in workflows/*.yml; do
            agentflow workflow debug "$workflow" --validate
          done
```

### Pre-commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

for workflow in workflows/*.yml; do
  if ! agentflow workflow debug "$workflow" --validate > /dev/null 2>&1; then
    echo "❌ Workflow validation failed: $workflow"
    exit 1
  fi
done
```

## Troubleshooting

### Empty Output

If no output is shown:
1. Check that the workflow file exists
2. Verify the YAML is valid
3. Ensure the file follows the V2 format

### Validation Errors Not Clear

Use verbose mode for more details:

```bash
agentflow workflow debug workflow.yml --validate --verbose
```

### Performance Issues

For very large workflows (>100 nodes), consider:
- Splitting into smaller sub-workflows
- Using `--validate` only (faster than full analysis)
- Restructuring to reduce complexity

## API for Programmatic Use

The debug functionality can also be used programmatically:

```rust
use agentflow_cli::commands::workflow::debug;

// Debug a workflow
debug::execute(
    "workflow.yml".to_string(),
    true,  // visualize
    false, // dry_run
    true,  // analyze
    true,  // validate
    true,  // plan
    false, // verbose
).await?;
```

## Future Enhancements

Planned improvements:
- 📈 Performance profiling integration
- 🎯 Interactive mode with node selection
- 📊 Export to Graphviz/Mermaid formats
- 🔍 Advanced dependency analysis
- 📝 Automated optimization suggestions

## Related Documentation

- [Workflow Configuration Guide](./CONFIGURATION.md)
- [Retry Mechanism](./RETRY_MECHANISM.md)
- [Short-term Improvements](./SHORT_TERM_IMPROVEMENTS.md)

---

**Last Updated**: 2025-10-26
**Version**: 0.2.0
