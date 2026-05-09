# AgentFlow Configuration

Last updated: 2026-05-09

This document covers the configuration that is currently implemented by the CLI:
model/provider configuration, secrets, workflow YAML, run directories, and the
main validation commands.

## Model Configuration

AgentFlow resolves model configuration with this priority:

1. `AGENTFLOW_MODELS_CONFIG`
2. `~/.agentflow/models.yml`
3. `~/.agentflow/models.yaml`
4. bundled `default_models.yml` when no user config exists

`models.yml` is the canonical filename. `models.yaml` is supported as a
legacy fallback. If both files exist, AgentFlow uses `models.yml` and prints a
warning.

Initialize local configuration with:

```bash
agentflow config init
```

This creates:

```text
~/.agentflow/models.yml
~/.agentflow/.env
```

Inspect and validate the active configuration with:

```bash
agentflow config show
agentflow config show models
agentflow config show providers
agentflow config validate
agentflow doctor
agentflow doctor --format json
agentflow llm models
agentflow llm models --provider openai --detailed
```

`config show`, `config validate`, `doctor`, and `llm models` all report or use
the same resolved model configuration source.

`agentflow llm` is limited to model discovery and diagnostics. Interactive model
use should go through `agentflow skill run`, `agentflow skill chat`, or
`agentflow workflow run`.

## Secrets

Do not store raw API keys in workflow YAML or `models.yml`. Store secrets in the
shell environment or in `~/.agentflow/.env`, and let model/provider entries refer
to the environment variable name.

Common variables:

```bash
OPENAI_API_KEY=...
ANTHROPIC_API_KEY=...
GEMINI_API_KEY=...
MOONSHOT_API_KEY=...
DASHSCOPE_API_KEY=...
STEPFUN_API_KEY=...
```

Recommended local permissions:

```bash
chmod 700 ~/.agentflow
chmod 600 ~/.agentflow/.env ~/.agentflow/models.yml
```

See [SECRET_MANAGEMENT.md](SECRET_MANAGEMENT.md) for the broader policy.

## Runtime Model Selection

Model selection precedence is:

1. CLI `--model` for the current command.
2. Node-level `parameters.model` in workflow YAML or `[model].name` in a Skill.
3. Built-in runtime default.

Supported overrides include:

```bash
agentflow workflow run flow.yml --model gpt-4o-mini
agentflow skill run ./skills/code-reviewer --message "review this" --model gpt-4o-mini
agentflow skill chat ./skills/code-reviewer --model gpt-4o-mini
```

## Workflow YAML

The current config-first workflow format is `FlowDefinitionV2`:

```yaml
name: "Example Workflow"
inputs:
  topic:
    description: "Topic to pass into the workflow"
    required: false
    default: "AgentFlow"
nodes:
  - id: render_prompt
    type: template
    parameters:
      template: "Write a short summary about {{topic}}."

  - id: summarize
    type: llm
    dependencies: ["render_prompt"]
    input_mapping:
      prompt: "{{ nodes.render_prompt.outputs.output }}"
    parameters:
      model: gpt-4o-mini
      temperature: 0.2
      max_tokens: 256
```

Top-level fields:

| Field | Required | Description |
| --- | --- | --- |
| `name` | Yes | Workflow name used in CLI output and validation reports. |
| `inputs` | No | Named workflow inputs with `description`, `required`, and `default`. |
| `nodes` | Yes | Ordered list of workflow node definitions. |

Node fields:

| Field | Required | Description |
| --- | --- | --- |
| `id` | Yes | Unique node id. |
| `type` | Yes | Node type supported by the CLI factory. |
| `dependencies` | No | Node ids that must complete before this node runs. |
| `input_mapping` | No | Runtime mappings from previous node outputs. |
| `run_if` | No | Conditional expression evaluated by the workflow runtime. |
| `parameters` | No | Node-specific parameter map. |

Supported mapping expressions currently use this form:

```yaml
input_mapping:
  prompt: "{{ nodes.render_prompt.outputs.output }}"
```

See [WORKFLOW_SCHEMA.md](WORKFLOW_SCHEMA.md) for the supported node types and
their required/optional parameters.

## Workflow Commands

Run a workflow:

```bash
agentflow workflow run flow.yml
agentflow workflow run flow.yml --dry-run
agentflow workflow run flow.yml --model gpt-4o-mini
agentflow workflow run flow.yml --execution-mode concurrent --max-concurrency 4
agentflow workflow run flow.yml --input topic AgentFlow
```

Validate without execution:

```bash
agentflow workflow validate flow.yml
agentflow workflow validate flow.yml --format json
agentflow workflow validate flow.yml --strict
```

Debug workflow structure:

```bash
agentflow workflow debug flow.yml --validate
agentflow workflow debug flow.yml --visualize
agentflow workflow debug flow.yml --analyze
agentflow workflow debug flow.yml --plan
agentflow workflow debug flow.yml --dry-run --verbose
```

`workflow run` and `workflow run --dry-run` both execute schema validation before
building the graph.

## Run And Trace Directories

Workflow run artifacts default to:

```text
~/.agentflow/runs
```

Override the base directory with:

```bash
agentflow workflow run flow.yml --run-dir /var/lib/agentflow/runs
AGENTFLOW_RUN_DIR=/tmp/agentflow-runs agentflow workflow run flow.yml
```

Trace files default to:

```text
~/.agentflow/traces
```

Inspect persisted traces with:

```bash
agentflow trace replay <run_id>
agentflow trace tui <run_id>
```

## Skills

Skills use `SKILL.md` as the recommended entry point, with `skill.toml` still
supported for explicit structured overrides. Skill commands include:

```bash
agentflow skill init ./my-skill --description "Describe this skill"
agentflow skill validate ./my-skill
agentflow skill inspect ./my-skill
agentflow skill list-tools ./my-skill
agentflow skill run ./my-skill --message "hello"
agentflow skill chat ./my-skill
agentflow skill test ./my-skill --dry-run
```

See [SKILLS.md](SKILLS.md), [SKILL_FORMAT.md](SKILL_FORMAT.md), and
[SKILL_REGISTRY.md](SKILL_REGISTRY.md).
