# Trace Persistence Schema

AgentFlow trace persistence uses a normalized relational schema plus a full
`trace_json` copy on `trace_runs` for compatibility with the existing
`ExecutionTrace` model.

The canonical DDL constants live in `agentflow-tracing/src/storage/schema.rs`:

- `POSTGRES_TRACE_SCHEMA`
- `SQLITE_TRACE_SCHEMA`
- `TRACE_SCHEMA_VERSION`

## Tables

- `trace_runs`: one workflow/agent execution run. Stores workflow status,
  metadata, timing, tags, and the full serialized trace.
- `trace_steps`: workflow node or agent-capable step records, linked to
  `trace_runs`.
- `trace_events`: structured workflow/agent/runtime events, optionally linked
  to a step.
- `trace_tool_calls`: builtin/script/MCP/workflow tool call records.
- `trace_mcp_calls`: MCP-specific server/tool request and response records,
  linked to a tool call when available.

## Dialects

Postgres uses `JSONB`, `TIMESTAMPTZ`, `BIGSERIAL`, and GIN indexing for tags.
SQLite uses `TEXT` JSON payloads and ISO timestamp strings. Both dialects keep
the same table and column names so query code can share most of its shape.

## Hop continuity (P3.8)

W3C `traceparent` is the wire-format AgentFlow uses to keep an OTel
trace stitched across every process and protocol hop a run touches.
Producers install the active value via
`agentflow_tracing::context::scope(traceparent, fut)` and consumers
read it with `agentflow_tracing::context::current_traceparent()`. The
canonical env var on outbound spawns is
`agentflow_tracing::context::TRACEPARENT_ENV` (`"TRACEPARENT"`).

| Hop | Carrier | Wired (✓) / planned (○) |
| --- | --- | --- |
| LLM HTTP call | `traceparent` HTTP header (via `agentflow_llm::trace_context::LlmTraceContext` task-local) | ✓ |
| Plugin subprocess spawn | `TRACEPARENT` env var, injected by `agentflow-cli` plugin preparers (`OsSandboxPluginPreparer` + `NoopWithTraceparent`) | ✓ |
| MCP transport (JSON-RPC stdio) | JSON-RPC `meta.traceparent` field | ○ follow-up |
| Worker gRPC | gRPC `traceparent` metadata entry | ○ follow-up |

When no context is in scope, `current_traceparent()` returns `None`
and consumers MUST NOT emit a carrier — propagating an empty value
would mask the "no upstream context" case for OTel-aware receivers.

## Next Implementation Step

The next storage implementation should add a relational `TraceStorage`
backend that:

1. Executes the selected schema at startup.
2. Upserts `trace_runs`.
3. Rewrites child rows for a run transactionally on `save_trace`.
4. Rehydrates `ExecutionTrace` from `trace_json` for compatibility while
   enabling indexed queries over normalized columns.
