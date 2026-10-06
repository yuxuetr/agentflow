# Yanshi CLI

This crate provides a powerful command-line interface (CLI) to interact with the Yanshi V2 engine.

## Installation

Build and install the CLI using Cargo:

```bash
cargo install --path yanshi-cli
```

## Usage

The CLI is structured around a series of commands and subcommands.

```bash
yanshi [COMMAND]
```

You can get help for any command or subcommand by using the `--help` flag.

```bash
yanshi --help
yanshi audio --help
yanshi audio tts --help
```

## Commands

Here is an overview of the main commands available.

### `workflow`

Orchestrate and execute complex, multi-node workflows defined in YAML files.

**Usage:**

```bash
# Run a workflow file
yanshi workflow run path/to/your/workflow.yml

# Preview the V2 DAG execution order without running nodes
yanshi workflow run path/to/your/workflow.yml --dry-run

# Validate schema and dependencies without execution
yanshi workflow validate path/to/your/workflow.yml

# Emit machine-readable validation output and fail on unknown parameters
yanshi workflow validate path/to/your/workflow.yml --format json --strict

# Inject CLI inputs, override LLM/skill-agent models, and save final state
yanshi workflow run path/to/your/workflow.yml \
  --input topic Yanshi \
  --model mock-model \
  --output result.json

# Run ready workflow nodes concurrently
yanshi workflow run path/to/your/workflow.yml \
  --execution-mode concurrent \
  --max-concurrency 4

# Inspect validation, structure, analysis, and execution plan
yanshi workflow debug path/to/your/workflow.yml --validate --plan --analyze
```

Current `workflow run` uses the V2 `FlowDefinitionV2 -> GraphNode -> yanshi_core::Flow`
path. Public flags are wired to behavior; `--watch` currently returns an explicit
not-implemented error instead of being ignored.

See `docs/WORKFLOW_SCHEMA.md` for the current node parameter contract.

### `audio`

Perform audio-related tasks like transcription and speech synthesis.

**Subcommands:**

-   `asr`: Transcribe an audio file to text.
-   `tts`: Synthesize speech from text.
-   `clone`: Clone a voice (not fully implemented).

**Usage Examples:**

```bash
# Transcribe an audio file
yanshi audio asr path/to/your/audio.mp3

# Synthesize a sentence and save it to an mp3 file
yanshi audio tts --voice nova --output hello.mp3 "Hello, world! This is Yanshi."
```

### `image`

Perform image generation and understanding tasks.

**Subcommands:**

-   `generate`: Create an image from a text prompt.
-   `understand`: Analyze an image with a text prompt.

**Usage Examples:**

```bash
# Generate an image and save it
yanshi image generate --prompt "A photorealistic cat wearing a wizard hat" --output wizard_cat.png

# Ask a question about an image
yanshi image understand --image path/to/your/image.jpg --text "What is the main subject of this image?"
```

### `llm`

Inspect configured language models. Yanshi's interactive path is agent-first;
use `skill chat`, `skill run`, or workflow `skill_agent` nodes for conversations.

**Subcommands:**

-   `models`: List available models.

**Usage Examples:**

```bash
# List all available models
yanshi llm models

# List models from a specific provider
yanshi llm models --provider openai
```

### `config`

Manage the Yanshi configuration.

**Subcommands:**

-   `init`: Create a default configuration file.
-   `show`: Display the current configuration.
-   `validate`: Validate the configuration files.

**Usage Examples:**

```bash
# Create a new config file if one doesn't exist
yanshi config init

# Show the current configuration
yanshi config show

# Show only configured models or validate env var availability
yanshi config show models
yanshi config validate
```

### `skill`

Create, install, inspect, test, and run agent Skills.

**Usage Examples:**

```bash
# Create and validate a Skill
yanshi skill init ./skills/code-reviewer
yanshi skill validate ./skills/code-reviewer

# Inspect model, tools, memory, MCP servers, knowledge, and security settings
yanshi skill inspect ./skills/code-reviewer

# Discover tools without running the agent
yanshi skill list-tools ./skills/code-reviewer
yanshi skill test ./skills/code-reviewer --dry-run

# Run with model override and trace output
yanshi skill run ./skills/code-reviewer \
  --message "Review this change" \
  --model gpt-4o-mini \
  --trace
```
