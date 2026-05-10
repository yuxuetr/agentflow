# Official Ecosystem Examples

This directory contains offline-first examples for the v1 ecosystem surface:
Skills, subprocess Plugins, a remote marketplace manifest, and a hybrid
workflow demo that ties DAG, Agent, MCP, RAG, Trace, and Web UI concepts
together.

## Contents

- `skills/`: official `SKILL.md` samples:
  - `code-reviewer`
  - `research-assistant`
  - `multimodal-content-analyzer`
- `plugins/`: official subprocess plugin samples:
  - `echo`
  - `data-transform`
- `marketplace/remote-marketplace.toml`: remote marketplace schema example.
- `workflows/hybrid_offline_demo.yml`: config-first hybrid workflow demo.

## Validate Samples

```bash
env CARGO_TARGET_DIR=/tmp/agentflow-target \
  cargo run -p agentflow-cli -- skill validate examples/ecosystem/skills/code-reviewer

env CARGO_TARGET_DIR=/tmp/agentflow-target \
  cargo run -p agentflow-cli --features plugin -- \
  plugin inspect examples/ecosystem/plugins/echo

env CARGO_TARGET_DIR=/tmp/agentflow-target \
  cargo run -p agentflow-cli -- \
  marketplace search examples/ecosystem/marketplace/remote-marketplace.toml
```

## Hybrid Demo

The hybrid workflow includes:

- deterministic DAG templating;
- a RAG search node;
- an MCP tool node;
- a subprocess plugin node;
- a Skill-backed agent node;
- trace and Web UI follow-up commands.

Validate the workflow without live services:

```bash
env CARGO_TARGET_DIR=/tmp/agentflow-target \
  cargo run -p agentflow-cli --features "mcp rag plugin" -- \
  workflow validate examples/ecosystem/workflows/hybrid_offline_demo.yml --strict
```

Dry-run the workflow shape:

```bash
env CARGO_TARGET_DIR=/tmp/agentflow-target \
  cargo run -p agentflow-cli --features "mcp rag plugin" -- \
  workflow run examples/ecosystem/workflows/hybrid_offline_demo.yml --dry-run
```

Full execution is opt-in because it requires a reachable MCP server, Qdrant
collection, and model configuration. For a live run, set provider config,
enable tracing, and point the RAG/MCP nodes at local services:

```bash
export AGENTFLOW_TRACE_DIR=/tmp/agentflow-traces
env CARGO_TARGET_DIR=/tmp/agentflow-target \
  cargo run -p agentflow-cli --features "mcp rag plugin" -- \
  workflow run examples/ecosystem/workflows/hybrid_offline_demo.yml \
  --model mock-model \
  --output /tmp/agentflow-hybrid-output.json
```

Then inspect the run through trace replay or the Web UI:

```bash
env CARGO_TARGET_DIR=/tmp/agentflow-target \
  cargo run -p agentflow-cli -- trace replay <run_id> --dir /tmp/agentflow-traces

env CARGO_TARGET_DIR=/tmp/agentflow-target \
  cargo run -p agentflow-server
```

Open `http://localhost:8080/ui` and paste the server run id when the workflow
was submitted through `/v1/runs`.
