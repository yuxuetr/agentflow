# AgentFlow CLI

Command-line interface for AgentFlow workflow orchestration and LLM interaction.

## Installation

Build from source:
```bash
cargo build --package agentflow-cli --release
```

The binary will be available at `$HOME/.target/release/agentflow`.

## Quick Start

### LLM Commands

```bash
# Send a prompt to an LLM
agentflow llm prompt "Write a haiku about programming" --model gpt-4o

# List available models
agentflow llm models

# Show detailed model information
agentflow llm models --detailed

# Filter models by provider
agentflow llm models --provider openai
```

### Workflow Commands (Coming Soon)

```bash
# Run a workflow from file
agentflow run workflow.yml --input topic="AI Ethics"

# Validate workflow configuration
agentflow validate workflow.yml

# List available templates
agentflow list workflows
```

## Configuration

AgentFlow CLI uses the same configuration system as the AgentFlow LLM library:

1. **Project-specific**: `./models.yml` (highest priority)
2. **User-specific**: `~/.agentflow/models.yml` (medium priority)
3. **Built-in defaults**: Bundled in crate (lowest priority)

### Initialize Configuration

```bash
# Initialize configuration files
agentflow config init

# Show current configuration
agentflow config show

# Validate configuration
agentflow config validate
```

## Workflow Templates

The CLI includes several workflow templates in the `templates/` directory:

- `simple.yml`: Basic text generation workflow
- `llm-chain.yml`: Multi-step LLM processing chain
- `parallel.yml`: Parallel processing example (planned)
- `conditional.yml`: Conditional branching example (planned)

## Development Status

### Phase 1: CLI Foundation ✅
- [x] CLI argument parsing with clap
- [x] LLM commands (`prompt`, `models`)
- [x] Basic project structure
- [x] Workspace integration

### Phase 2: Workflow Engine (In Progress)
- [ ] YAML workflow parser
- [ ] Core node types (LLM, template, file, HTTP)
- [ ] Sequential workflow execution
- [ ] Workflow validation

### Phase 3: Advanced Features (Planned)
- [ ] Parallel workflow execution
- [ ] Conditional branching
- [ ] Loop workflows
- [ ] Batch processing
- [ ] Interactive chat mode
- [ ] Streaming output

### Phase 4: Integration and Polish (Planned)
- [ ] MCP client integration
- [ ] Rich error messages
- [ ] Progress indicators
- [ ] Performance optimization

## Examples

### Simple Text Generation

```bash
agentflow llm prompt "Explain quantum computing in simple terms" \
  --model gpt-4o \
  --temperature 0.7 \
  --max-tokens 300 \
  --output explanation.txt
```

### File Input

```bash
agentflow llm prompt "Analyze this code" \
  --file src/main.rs \
  --model claude-3-sonnet
```

### Workflow Execution (Coming Soon)

```bash
# Run simple text generation workflow
agentflow run templates/simple.yml \
  --input prompt="Write a technical blog post about Rust" \
  --input model="gpt-4o" \
  --output blog_post.txt

# Run LLM chain workflow  
agentflow run templates/llm-chain.yml \
  --input topic="Quantum Computing Applications" \
  --input depth="comprehensive"
```

## Global Options

- `--log-level <LEVEL>`: Set logging level (error, warn, info, debug, trace)
- `--output-format <FORMAT>`: Output format (json, yaml, text)
- `--no-color`: Disable colored output
- `--verbose`: Enable verbose output

## Architecture

The CLI is built on top of:
- **agentflow-core**: Core workflow execution engine
- **agentflow-llm**: Unified LLM interface
- **clap**: Command-line argument parsing
- **tokio**: Async runtime
- **serde**: Configuration serialization
- **tera**: Template rendering

## Contributing

1. Implement new workflow node types in `src/executor/nodes/`
2. Add new commands in `src/commands/`
3. Extend workflow configuration schema in `src/config/`
4. Add workflow templates in `templates/`

## License

MIT License - see the main AgentFlow repository for details.