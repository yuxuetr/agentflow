# CLI Config-First Tutorial

This tutorial exercises the current CLI path without requiring external API
keys. It uses the built-in mock provider for model calls.

## 1. Configure A Mock Model

```bash
mkdir -p ~/.yanshi
cat > ~/.yanshi/models.yml <<'YAML'
models:
  mock-model:
    vendor: mock
    type: text
    model_id: mock-model
providers:
  mock:
    api_key_env: MOCK_API_KEY
YAML

yanshi config show models
yanshi config validate
yanshi llm models --provider mock --detailed
```

## 2. Run A Fixed DAG

```bash
yanshi workflow run yanshi-cli/examples/workflows/fixed_dag_basic.yml --dry-run

yanshi workflow run yanshi-cli/examples/workflows/fixed_dag_basic.yml \
  --input topic Yanshi \
  --output /tmp/yanshi-fixed-dag.json
```

## 3. Inspect And Test A Skill

```bash
yanshi skill inspect yanshi-cli/examples/skills/mock-reviewer
yanshi skill list-tools yanshi-cli/examples/skills/mock-reviewer
yanshi skill test yanshi-cli/examples/skills/mock-reviewer --dry-run
```

## 4. Run A Skill With Model And Memory Overrides

```bash
YANSHI_MOCK_RESPONSE='{"thought":"done","answer":"Reviewed with mock model."}' \
  yanshi skill run yanshi-cli/examples/skills/mock-reviewer \
    --message "Review the CLI workflow changes" \
    --model mock-model \
    --memory none \
    --trace
```

## 5. Run A Skill-Agent Workflow

```bash
yanshi workflow run yanshi-cli/examples/workflows/skill_agent_hybrid.yml --dry-run

YANSHI_MOCK_RESPONSE='{"thought":"done","answer":"Looks good."}' \
  yanshi workflow run yanshi-cli/examples/workflows/skill_agent_hybrid.yml \
    --model mock-model \
    --output /tmp/yanshi-skill-agent.json
```

The final state JSON contains the skill-agent `response`, `session_id`,
`agent_result`, and `agent_resume` fields.

## 6. Dry-Run A RAG + Skill Workflow

This verifies the config-first shape for a workflow that searches a RAG
collection and then passes retrieved context into a Skill-backed agent. The
dry-run path does not contact Qdrant or an embedding provider.

```bash
cargo run -p yanshi-cli --features rag -- \
  workflow run yanshi-cli/examples/workflows/rag_skill_assistant.yml --dry-run
```

Full execution additionally requires Qdrant, embedding credentials such as
`OPENAI_API_KEY`, and a configured chat model.

## 7. Marketplace Install Flow

```bash
yanshi skill marketplace list yanshi-skills/examples/marketplace.toml
yanshi skill marketplace install yanshi-skills/examples/marketplace.toml mcp-demo \
  --dir /tmp/yanshi-skills \
  --force
yanshi skill inspect /tmp/yanshi-skills/mcp-basic
```

## 8. Trace Viewing

When a command writes trace files, inspect them with:

```bash
yanshi trace replay <run_id>
yanshi trace tui <run_id> --filter all --details
```
