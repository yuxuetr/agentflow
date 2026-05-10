# AgentFlow Stability Boundaries

This document is the v1 stability inventory for public extension points,
wire schemas, and manifests. It describes what downstream users can depend on
and where AgentFlow still reserves room to change behavior before v1.

## Stability Levels

| Level | Meaning |
| --- | --- |
| Stable | Public contract. Breaking changes require a major version or an explicit migration window. |
| Beta | Public and supported, but may receive additive or narrowly breaking changes before v1. |
| Experimental | Available for evaluation. Do not build long-lived integrations without pinning versions. |
| Internal | Not a public compatibility promise. |

## Public Rust Extension Points

| Surface | Crate | Level | Compatibility promise |
| --- | --- | --- | --- |
| `AsyncNode` | `agentflow-core` | Stable | Implementors provide `async fn execute(&self, &AsyncNodeInputs) -> AsyncNodeResult`. Inputs and outputs remain `HashMap<String, FlowValue>`. New helper traits or blanket adapters may be added, but this method signature is the v1 contract. |
| `FlowValue` | `agentflow-core` | Stable | Variants `Json`, `File { path, mime_type }`, and `Url { url, mime_type }` are stable. Serialization uses the tagged `type` schema described below. New variants require a schema-versioned migration or backwards-compatible fallback. |
| `Tool` | `agentflow-tools` | Stable | Tool implementors expose `name`, `description`, `parameters_schema`, optional metadata/idempotency/capabilities, and `execute(Value) -> ToolOutput`. Required methods are stable for v1. |
| `ToolMetadata` | `agentflow-tools` | Stable | Fields `source`, `permissions`, `idempotency`, `mcp_server_name`, and `mcp_tool_name` are stable. New optional fields may be added with serde defaults. |
| `AgentRuntime` | `agentflow-agents` | Stable | Custom runtimes implement `run(AgentContext) -> AgentRunResult` and `runtime_name()`. Implementors must honor runtime limits, cancellation, and chronological steps. |
| `AgentStep` / `AgentEvent` | `agentflow-agents` | Stable closed schema | The serialized tagged enums are stable for trace replay. The enums are intentionally closed; new variants are allowed only in AgentFlow releases and must be documented as additive schema changes. |

## Workflow and Checkpoint Schemas

### `FlowValue` Checkpoint Schema

Checkpointed node outputs use a tagged object:

```json
{ "type": "json", "value": { "answer": 42 } }
{ "type": "file", "path": "/tmp/out.png", "mime_type": "image/png" }
{ "type": "url", "url": "https://example.test/a.png", "mime_type": "image/png" }
```

Compatibility rules:

- Readers must continue accepting legacy raw JSON checkpoint values as
  `FlowValue::Json`.
- Writers must emit the tagged `type` field.
- `mime_type` may be `null`.
- Paths are stored as strings and interpreted by the runtime that resumes the run.

### Workflow YAML

The config-first workflow schema is Beta. The validator contract in
`docs/WORKFLOW_SCHEMA.md` is the source of truth. Node types and optional
parameters may grow additively; removing a node type or changing the meaning of
an existing required field is breaking.

## Skill and Plugin Manifests

| Surface | Source type | Level | Compatibility promise |
| --- | --- | --- | --- |
| `SKILL.md` frontmatter | `agentflow-skills::skill_md` | Stable | Required `name` and `description`; optional `license`, `compatibility`, `metadata`, `mcp_servers`, `security`, and `allowed-tools`. Unknown frontmatter keys are ignored. |
| `skill.toml` | `agentflow-skills::SkillManifest` | Stable | `skill`, `persona`, `model`, `security`, `tools`, `mcp_servers`, `knowledge`, and `memory` sections are stable. New optional fields must have defaults. When both files exist, `skill.toml` wins. |
| Plugin manifest | `agentflow-core::plugin::PluginManifest` | Beta | `plugin.toml` with `[plugin]`, `runtime`, `entrypoint`, `protocol`, `nodes`, and `capabilities`. Protocol `agentflow.plugin/1` is the current wire version. |
| Marketplace manifest | `agentflow-skills::RemoteMarketplaceManifest` | Beta | `schema_version`, registry metadata, and `entries` with package type, source, checksum, signature, aliases, and description are the current remote registry contract. |

## Trace and Server APIs

| Surface | Level | Compatibility promise |
| --- | --- | --- |
| Trace persistence schema | Beta | Table names and core columns in `docs/TRACE_PERSISTENCE_SCHEMA.md` are the compatibility target. New columns/tables may be added. Existing columns must not change type without a migration. |
| Server REST API envelope | Beta | JSON response field names for `/v1/runs`, `/v1/runs/{id}`, `/v1/runs/{id}/graph`, `/v1/runs/{id}/events/history`, `/v1/skills`, and marketplace-related commands are preserved. Error responses use the server `ApiError` envelope. |
| SSE event envelope | Beta | Events carry `run_id`, `seq`, `kind`, `payload`, and `ts`. `seq` is monotonically increasing per run and clients may reconnect with `after_seq`. New `kind` values are additive. |

## Non-Stable Surfaces

The following are intentionally not v1-stable yet:

- Internal scheduler structs used only by `agentflow-server`.
- Web UI DOM/CSS class names.
- Exact CLI human-readable output. JSON output modes are the preferred
  automation surface.
- Experimental RAG evaluation metrics and datasets.
- Future WASM plugin runtime details beyond the reserved `runtime = "wasm"`
  manifest value.
