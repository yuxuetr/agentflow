# CLI Config-First Tutorial

This tutorial exercises the current CLI path without requiring external API
keys. It uses the built-in mock provider for model calls.

## 1. Configure A Mock Model

```bash
mkdir -p ~/.agentflow
cat > ~/.agentflow/models.yml <<'YAML'
models:
  mock-model:
    vendor: mock
    type: text
    model_id: mock-model
providers:
  mock:
    api_key_env: MOCK_API_KEY
YAML

agentflow config show models
agentflow config validate
agentflow llm models --provider mock --detailed
```

## 2. Run A Fixed DAG

```bash
agentflow workflow run agentflow-cli/examples/workflows/fixed_dag_basic.yml --dry-run

agentflow workflow run agentflow-cli/examples/workflows/fixed_dag_basic.yml \
  --input topic AgentFlow \
  --output /tmp/agentflow-fixed-dag.json
```

## 3. Inspect And Test A Skill

```bash
agentflow skill inspect agentflow-cli/examples/skills/mock-reviewer
agentflow skill list-tools agentflow-cli/examples/skills/mock-reviewer
agentflow skill test agentflow-cli/examples/skills/mock-reviewer --dry-run
```

## 4. Run A Skill With Model And Memory Overrides

```bash
AGENTFLOW_MOCK_RESPONSE='{"thought":"done","answer":"Reviewed with mock model."}' \
  agentflow skill run agentflow-cli/examples/skills/mock-reviewer \
    --message "Review the CLI workflow changes" \
    --model mock-model \
    --memory none \
    --trace
```

## 5. Run A Skill-Agent Workflow

```bash
agentflow workflow run agentflow-cli/examples/workflows/skill_agent_hybrid.yml --dry-run

AGENTFLOW_MOCK_RESPONSE='{"thought":"done","answer":"Looks good."}' \
  agentflow workflow run agentflow-cli/examples/workflows/skill_agent_hybrid.yml \
    --model mock-model \
    --output /tmp/agentflow-skill-agent.json
```

The final state JSON contains the skill-agent `response`, `session_id`,
`agent_result`, and `agent_resume` fields.

## 6. Dry-Run A RAG + Skill Workflow

This verifies the config-first shape for a workflow that searches a RAG
collection and then passes retrieved context into a Skill-backed agent. The
dry-run path does not contact Qdrant or an embedding provider.

```bash
cargo run -p agentflow-cli --features rag -- \
  workflow run agentflow-cli/examples/workflows/rag_skill_assistant.yml --dry-run
```

Full execution additionally requires Qdrant, embedding credentials such as
`OPENAI_API_KEY`, and a configured chat model.

## 7. Marketplace Install Flow

```bash
agentflow skill marketplace list agentflow-skills/examples/marketplace.toml
agentflow skill marketplace install agentflow-skills/examples/marketplace.toml mcp-demo \
  --dir /tmp/agentflow-skills \
  --force
agentflow skill inspect /tmp/agentflow-skills/mcp-basic
```

## 8. Trace Viewing

When a command writes trace files, inspect them with:

```bash
agentflow trace replay <run_id>
agentflow trace tui <run_id> --filter all --details
```
