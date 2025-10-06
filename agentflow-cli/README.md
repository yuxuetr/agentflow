# AgentFlow CLI

This crate provides a powerful command-line interface (CLI) to interact with the AgentFlow V2 engine.

## Installation

Build and install the CLI using Cargo:

```bash
cargo install --path agentflow-cli
```

## Usage

The CLI is structured around a series of commands and subcommands.

```bash
agentflow [COMMAND]
```

You can get help for any command or subcommand by using the `--help` flag.

```bash
agentflow --help
agentflow audio --help
agentflow audio tts --help
```

## Commands

Here is an overview of the main commands available.

### `workflow`

Orchestrate and execute complex, multi-node workflows defined in YAML files.

**Usage:**

```bash
# Run a workflow file
agentflow workflow run path/to/your/workflow.yml
```

### `audio`

Perform audio-related tasks like transcription and speech synthesis.

**Subcommands:**

-   `asr`: Transcribe an audio file to text.
-   `tts`: Synthesize speech from text.
-   `clone`: Clone a voice (not fully implemented).

**Usage Examples:**

```bash
# Transcribe an audio file
agentflow audio asr path/to/your/audio.mp3

# Synthesize a sentence and save it to an mp3 file
agentflow audio tts --voice nova --output hello.mp3 "Hello, world! This is AgentFlow."
```

### `image`

Perform image generation and understanding tasks.

**Subcommands:**

-   `generate`: Create an image from a text prompt.
-   `understand`: Analyze an image with a text prompt.

**Usage Examples:**

```bash
# Generate an image and save it
agentflow image generate --prompt "A photorealistic cat wearing a wizard hat" --output wizard_cat.png

# Ask a question about an image
agentflow image understand --image path/to/your/image.jpg --text "What is the main subject of this image?"
```

### `llm`

Directly interact with language models.

**Subcommands:**

-   `chat`: Start an interactive chat session (implementation pending).
-   `models`: List available models.

**Usage Examples:**

```bash
# List all available models
agentflow llm models

# List models from a specific provider
agentflow llm models --provider openai
```

### `config`

Manage the AgentFlow configuration.

**Subcommands:**

-   `init`: Create a default configuration file.
-   `show`: Display the current configuration.
-   `validate`: Validate the configuration files.

**Usage Examples:**

```bash
# Create a new config file if one doesn't exist
agentflow config init

# Show the current configuration
agentflow config show
```
