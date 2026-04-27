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

## Next Implementation Step

The next storage implementation should add a relational `TraceStorage`
backend that:

1. Executes the selected schema at startup.
2. Upserts `trace_runs`.
3. Rewrites child rows for a run transactionally on `save_trace`.
4. Rehydrates `ExecutionTrace` from `trace_json` for compatibility while
   enabling indexed queries over normalized columns.
