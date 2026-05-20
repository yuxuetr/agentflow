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
| MCP server (`agentflow-mcp::server`) | Beta | Closed method set `initialize` / `notifications/initialized` / `tools/list` / `tools/call`; new methods may be added in minor releases, existing methods stay wire-stable. Required response fields: `initialize` → `result.protocolVersion` + `result.capabilities` + `result.serverInfo.{name,version}`; `tools/list` → `result.tools[]` with per-item `{name, description, input_schema}`; `tools/call` success → `result.content`, failure → `error.{code,message}` envelope. Notifications return no response. Error codes: `-32601` method-not-found, `-32603` tool-execution-failed (mirrors JSON-RPC + the broader `JsonRpcErrorCode` enum). `STABLE_PROTOCOL_VERSION = "2024-11-05"` — bumping breaks Beta. `MCPServer::handle_request` is the single public entry point; transports beyond stdio drive it directly. Promoted to Beta after P10.5.2 added fixture-based compat tests in `agentflow-mcp/tests/server_contracts.rs`. |
| Trace persistence schema | Beta | Table names and core columns in `docs/TRACE_PERSISTENCE_SCHEMA.md` are the compatibility target. New columns/tables may be added. Existing columns must not change type without a migration. |
| Server REST API envelope | Beta | JSON response field names for `/v1/runs`, `/v1/runs/{id}`, `/v1/runs/{id}/events/history`, `/v1/skills`, and marketplace-related commands are preserved. Error responses use the server `ApiError` envelope. (The previously-stable `/v1/runs/{id}/graph` was removed with the `agentflow-viz` crate deletion in P10.13.1.) |
| SSE event envelope | Beta | Events carry `run_id`, `seq`, `kind`, `payload`, and `ts`. `seq` is monotonically increasing per run and clients may reconnect with `after_seq`. New `kind` values are additive. |
| `ResumePlan` envelope | Beta | `agentflow-core::resume::ResumePlan` (`schema_version = 1`). Stable fields: `workflow_id`, `last_completed_node`, `status`, `created_at`, `tool_calls[]` (`node_id`, `tool_call_id`, `tool`, `step_index`, `idempotency`, `decision`, `reason`, `has_recorded_result`), `summary`, `force_replay`. Closed enums: `ResumeDecision` (`replay` / `skip` / `requires_manual`) and `ResumeIdempotency` (`idempotent` / `non_idempotent` / `unknown`). Surfaced by CLI `agentflow workflow resume-plan` and HTTP `GET /v1/runs/{id}/resume-plan`. |
| `HarnessEvent` envelope | Beta | `agentflow-harness` line-delimited JSON: `seq`, `session_id`, `ts`, `kind`, `payload`. Closed kind set (`session_started`, `step_started`, `tool_call_requested`, `approval_requested`, `approval_decided`, `tool_call_completed`, `background_task_updated`, `memory_summary_added`, `stopped`). Wire schema version `harness/1`. Additive optional fields and additive kinds keep the version; breaking changes bump it. Promoted to Beta after P-H.2 wired the envelopes through the in-process hook runtime and P-H.5 slice 2 surfaced them over `/v1/harness/sessions/{id}/events` (SSE) and `/events/history` (JSON). |
| `ApprovalRequest` / `ApprovalDecision` | Beta | `agentflow-harness` approval protocol envelopes. `id` joins request to decision. `decision` ∈ `allow` / `deny` / `deny_and_stop`. `scope` ∈ `once` / `session` / `run`. Promoted to Beta after P-H.5 slice 2 plumbed both shapes through `GET /v1/harness/sessions/{id}/approvals` and `POST .../{request_id}`. |
| CLI JSON envelope (`CliJsonEnvelope`) | Stable | Wire schema `agentflow.cli/1`. Closed four-field shape (`version`, `command`, `result`, `errors[]`). Top-level field set is closed; per-command `result` payloads follow the P0.3 additive-field contract. Bumping `version` requires a breaking-change RFC. Reference impl + round-trip tests in `agentflow-cli/src/json_envelope.rs`; full contract in `docs/CLI_JSON_OUTPUT.md`. Per-command migration plan tracked under P3.3 follow-ups in `TODOs.md`. |
| Distributed worker control plane (`WorkerProtocol`, `WorkerControlPlane`, `AuthenticatedControlPlane`, `WorkerCredential`, `WorkerAdmissionPolicy`, `NodeExecutionPayload`) | Experimental | `agentflow-server::scheduler` exposes the worker transport (gRPC), admission policy (allowlist + per-worker pre-shared keys + fleet / per-worker concurrency caps), and the portable `NodeExecutionPayload` schema as of P2.8/P5.5. Wire shape, admission semantics, and the supported-`node_type` table are subject to additive changes until the surface graduates to Beta (target: end of N10). Signed-JWT identity, JWT key rotation, and gRPC-metadata propagation of admission tokens are intentionally deferred — pin the worker minor version to avoid surprises. See `docs/DISTRIBUTED.md` for the full contract. |

## Compatibility Fixture Ownership

Golden fixtures for stable and beta serialized contracts live with the crate
that owns the Rust type or wire endpoint. Test modules may share helper
builders, but each owner is responsible for preserving backwards-compatible
reads and stable writer output for its rows below.

| Contract | Stability | Owner | Fixture location | Coverage |
| --- | --- | --- | --- | --- |
| `FlowValue` | Stable | `agentflow-core` | `agentflow-core/tests/fixtures/flow_value/` | Tagged `Json`, `File`, and `Url` values, `mime_type = null`, and legacy raw JSON values read as `FlowValue::Json`. |
| Checkpoint state | Stable for `FlowValue` payloads, Beta for surrounding checkpoint metadata | `agentflow-core` | `agentflow-core/tests/fixtures/checkpoints/` | Persisted node-output maps, checkpoint round trips, and verification that new writers keep emitting tagged `FlowValue` values. See [`docs/CHECKPOINT_SCHEMA.md`](CHECKPOINT_SCHEMA.md) for the warn-vs-silent fallback contract. |
| `AgentStep` | Stable closed schema | `agentflow-agents` | `agentflow-agents/tests/fixtures/agent_steps/` | Observe, plan, tool call, tool result with typed parts, reflection, final answer, handoff, blackboard, debate proposal, and debate verdict steps. |
| `AgentEvent` | Stable closed schema | `agentflow-agents` | `agentflow-agents/tests/fixtures/agent_events/` | Run lifecycle, tool policy, tool capability, memory hook derived events where serialized, handoff, blackboard, debate, and stop-reason payloads. |
| `ToolDefinition` and OpenAI-style tools array | Stable | `agentflow-tools` | `agentflow-tools/tests/fixtures/tool_contracts/` | Function name, description, JSON schema parameters, metadata defaults, and provider-facing `tools` array shape. |
| `ToolMetadata`, `ToolPermissionSet`, `ToolIdempotency`, `ToolOutputPart` | Stable | `agentflow-tools` | `agentflow-tools/tests/fixtures/tool_contracts/` | Builtin/script/MCP/workflow sources, sorted permission sets, idempotent/non-idempotent/unknown replay classes, and text/image/resource output parts. |
| `SKILL.md` frontmatter | Stable | `agentflow-skills` | `agentflow-skills/tests/fixtures/manifests/skill_md/` | Required `name` and `description`, optional compatibility/security/MCP/tool fields, and ignored unknown frontmatter keys. |
| `skill.toml` | Stable | `agentflow-skills` | `agentflow-skills/tests/fixtures/manifests/skill_toml/` | Stable manifest sections, serde defaults for new optional fields, and `skill.toml` precedence over `SKILL.md`. |
| Plugin manifest | Beta | `agentflow-core` | `agentflow-core/tests/fixtures/plugin_manifests/` | `plugin.toml` runtime, entrypoint, protocol, node declarations, capabilities, unsupported runtime failures, and protocol mismatch failures. |
| Marketplace manifest | Beta | `agentflow-skills` | `agentflow-skills/tests/fixtures/marketplace/` | Registry metadata, Skill and Plugin entries, source checksum/signature fields, aliases, and unknown optional field handling. |
| MCP server wire contract | Beta | `agentflow-mcp` | `agentflow-mcp/tests/fixtures/server_contracts/` | `initialize` / `notifications/initialized` / `tools/list` / `tools/call` request/response shapes, `STABLE_PROTOCOL_VERSION` round-trip, success vs error envelopes, JSON-RPC error codes (`-32601` / `-32603`), additive-field tolerance. |
| Trace persistence events | Beta | `agentflow-tracing` | `agentflow-tracing/tests/fixtures/trace_events/` | File/JSON trace event persistence, replay fixtures, trace status transitions, node/LLM/agent/tool details, and redaction-sensitive fields. |
| Server REST envelopes | Beta | `agentflow-server` | `agentflow-server/tests/fixtures/rest_envelopes/` | `/v1/runs`, `/v1/runs/{id}`, `/v1/runs/{id}/events/history`, success envelopes, pagination fields, and `ApiError` envelopes. |
| SSE events | Beta | `agentflow-server` | `agentflow-server/tests/fixtures/sse_events/` | `run_id`, monotonic `seq`, `kind`, `payload`, `ts`, backfill after `after_seq`, lag comments, and terminal event delivery. |
| `HarnessEvent` / `ApprovalRequest` / `ApprovalDecision` | Experimental | `agentflow-harness` | `agentflow-harness/tests/fixtures/` | Closed kind set, session/approval envelopes, additive optional-field tolerance, and `HARNESS_ENVELOPE_SCHEMA_VERSION` ("harness/1") stability. |

## Non-Stable Surfaces

The following are intentionally not v1-stable yet:

- Internal scheduler structs used only by `agentflow-server`.
- Web UI DOM/CSS class names.
- Exact CLI human-readable output. JSON output modes are the preferred
  automation surface.
- Experimental RAG evaluation metrics and datasets.
- Future WASM plugin runtime details beyond the reserved `runtime = "wasm"`
  manifest value.
