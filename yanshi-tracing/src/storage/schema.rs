//! Relational trace persistence schema.
//!
//! These DDL statements define the durable storage boundary for workflow,
//! agent, tool, and MCP traces. Storage backends can execute the matching
//! dialect-specific migration before implementing [`TraceStorage`](super::TraceStorage).

pub const TRACE_SCHEMA_VERSION: i32 = 1;

pub const POSTGRES_TRACE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS trace_runs (
  run_id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL,
  workflow_name TEXT,
  status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
  error TEXT,
  started_at TIMESTAMPTZ NOT NULL,
  completed_at TIMESTAMPTZ,
  duration_ms BIGINT,
  user_id TEXT,
  session_id TEXT,
  environment TEXT NOT NULL DEFAULT 'development',
  tags JSONB NOT NULL DEFAULT '[]'::jsonb,
  trace_json JSONB NOT NULL,
  schema_version INTEGER NOT NULL DEFAULT 1,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS trace_steps (
  id BIGSERIAL PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES trace_runs(run_id) ON DELETE CASCADE,
  node_id TEXT NOT NULL,
  node_type TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed', 'skipped')),
  started_at TIMESTAMPTZ NOT NULL,
  completed_at TIMESTAMPTZ,
  duration_ms BIGINT,
  input_json JSONB,
  output_json JSONB,
  error TEXT,
  agent_session_id TEXT,
  step_json JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS trace_events (
  id BIGSERIAL PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES trace_runs(run_id) ON DELETE CASCADE,
  step_id BIGINT REFERENCES trace_steps(id) ON DELETE CASCADE,
  event_type TEXT NOT NULL,
  event_time TIMESTAMPTZ NOT NULL,
  event_json JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS trace_tool_calls (
  id BIGSERIAL PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES trace_runs(run_id) ON DELETE CASCADE,
  step_id BIGINT REFERENCES trace_steps(id) ON DELETE CASCADE,
  tool_name TEXT NOT NULL,
  tool_source TEXT NOT NULL DEFAULT 'unknown',
  is_mcp BOOLEAN NOT NULL DEFAULT FALSE,
  params_json JSONB,
  result_json JSONB,
  is_error BOOLEAN,
  duration_ms BIGINT,
  started_at TIMESTAMPTZ,
  completed_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS trace_mcp_calls (
  id BIGSERIAL PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES trace_runs(run_id) ON DELETE CASCADE,
  tool_call_id BIGINT REFERENCES trace_tool_calls(id) ON DELETE CASCADE,
  server_name TEXT NOT NULL,
  remote_tool_name TEXT NOT NULL,
  request_json JSONB,
  response_json JSONB,
  is_error BOOLEAN NOT NULL DEFAULT FALSE,
  error TEXT,
  duration_ms BIGINT,
  started_at TIMESTAMPTZ,
  completed_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_trace_runs_workflow_id ON trace_runs(workflow_id);
CREATE INDEX IF NOT EXISTS idx_trace_runs_status ON trace_runs(status);
CREATE INDEX IF NOT EXISTS idx_trace_runs_started_at ON trace_runs(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_trace_runs_user_id ON trace_runs(user_id);
CREATE INDEX IF NOT EXISTS idx_trace_runs_tags ON trace_runs USING GIN(tags);
CREATE INDEX IF NOT EXISTS idx_trace_steps_run_id ON trace_steps(run_id);
CREATE INDEX IF NOT EXISTS idx_trace_steps_node_id ON trace_steps(node_id);
CREATE INDEX IF NOT EXISTS idx_trace_events_run_id ON trace_events(run_id);
CREATE INDEX IF NOT EXISTS idx_trace_events_type ON trace_events(event_type);
CREATE INDEX IF NOT EXISTS idx_trace_tool_calls_run_id ON trace_tool_calls(run_id);
CREATE INDEX IF NOT EXISTS idx_trace_tool_calls_tool_name ON trace_tool_calls(tool_name);
CREATE INDEX IF NOT EXISTS idx_trace_mcp_calls_run_id ON trace_mcp_calls(run_id);
CREATE INDEX IF NOT EXISTS idx_trace_mcp_calls_server_tool
  ON trace_mcp_calls(server_name, remote_tool_name);
"#;

pub const SQLITE_TRACE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS trace_runs (
  run_id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL,
  workflow_name TEXT,
  status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
  error TEXT,
  started_at TEXT NOT NULL,
  completed_at TEXT,
  duration_ms INTEGER,
  user_id TEXT,
  session_id TEXT,
  environment TEXT NOT NULL DEFAULT 'development',
  tags TEXT NOT NULL DEFAULT '[]',
  trace_json TEXT NOT NULL,
  schema_version INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS trace_steps (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL REFERENCES trace_runs(run_id) ON DELETE CASCADE,
  node_id TEXT NOT NULL,
  node_type TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed', 'skipped')),
  started_at TEXT NOT NULL,
  completed_at TEXT,
  duration_ms INTEGER,
  input_json TEXT,
  output_json TEXT,
  error TEXT,
  agent_session_id TEXT,
  step_json TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS trace_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL REFERENCES trace_runs(run_id) ON DELETE CASCADE,
  step_id INTEGER REFERENCES trace_steps(id) ON DELETE CASCADE,
  event_type TEXT NOT NULL,
  event_time TEXT NOT NULL,
  event_json TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS trace_tool_calls (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL REFERENCES trace_runs(run_id) ON DELETE CASCADE,
  step_id INTEGER REFERENCES trace_steps(id) ON DELETE CASCADE,
  tool_name TEXT NOT NULL,
  tool_source TEXT NOT NULL DEFAULT 'unknown',
  is_mcp INTEGER NOT NULL DEFAULT 0,
  params_json TEXT,
  result_json TEXT,
  is_error INTEGER,
  duration_ms INTEGER,
  started_at TEXT,
  completed_at TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS trace_mcp_calls (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL REFERENCES trace_runs(run_id) ON DELETE CASCADE,
  tool_call_id INTEGER REFERENCES trace_tool_calls(id) ON DELETE CASCADE,
  server_name TEXT NOT NULL,
  remote_tool_name TEXT NOT NULL,
  request_json TEXT,
  response_json TEXT,
  is_error INTEGER NOT NULL DEFAULT 0,
  error TEXT,
  duration_ms INTEGER,
  started_at TEXT,
  completed_at TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_trace_runs_workflow_id ON trace_runs(workflow_id);
CREATE INDEX IF NOT EXISTS idx_trace_runs_status ON trace_runs(status);
CREATE INDEX IF NOT EXISTS idx_trace_runs_started_at ON trace_runs(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_trace_runs_user_id ON trace_runs(user_id);
CREATE INDEX IF NOT EXISTS idx_trace_steps_run_id ON trace_steps(run_id);
CREATE INDEX IF NOT EXISTS idx_trace_steps_node_id ON trace_steps(node_id);
CREATE INDEX IF NOT EXISTS idx_trace_events_run_id ON trace_events(run_id);
CREATE INDEX IF NOT EXISTS idx_trace_events_type ON trace_events(event_type);
CREATE INDEX IF NOT EXISTS idx_trace_tool_calls_run_id ON trace_tool_calls(run_id);
CREATE INDEX IF NOT EXISTS idx_trace_tool_calls_tool_name ON trace_tool_calls(tool_name);
CREATE INDEX IF NOT EXISTS idx_trace_mcp_calls_run_id ON trace_mcp_calls(run_id);
CREATE INDEX IF NOT EXISTS idx_trace_mcp_calls_server_tool
  ON trace_mcp_calls(server_name, remote_tool_name);
"#;

pub fn schema_for_dialect(dialect: TraceSchemaDialect) -> &'static str {
  match dialect {
    TraceSchemaDialect::Postgres => POSTGRES_TRACE_SCHEMA,
    TraceSchemaDialect::Sqlite => SQLITE_TRACE_SCHEMA,
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceSchemaDialect {
  Postgres,
  Sqlite,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn schemas_include_required_trace_tables() {
    for schema in [POSTGRES_TRACE_SCHEMA, SQLITE_TRACE_SCHEMA] {
      for table in [
        "trace_runs",
        "trace_steps",
        "trace_events",
        "trace_tool_calls",
        "trace_mcp_calls",
      ] {
        assert!(
          schema.contains(&format!("CREATE TABLE IF NOT EXISTS {table}")),
          "schema should include {table}"
        );
      }
    }
  }

  #[test]
  fn schemas_preserve_run_foreign_keys_for_child_tables() {
    for schema in [POSTGRES_TRACE_SCHEMA, SQLITE_TRACE_SCHEMA] {
      assert!(schema.contains("REFERENCES trace_runs(run_id) ON DELETE CASCADE"));
      assert!(schema.contains("REFERENCES trace_steps(id) ON DELETE CASCADE"));
      assert!(schema.contains("REFERENCES trace_tool_calls(id) ON DELETE CASCADE"));
    }
  }

  #[test]
  fn dialect_selector_returns_matching_schema() {
    assert_eq!(
      schema_for_dialect(TraceSchemaDialect::Postgres),
      POSTGRES_TRACE_SCHEMA
    );
    assert_eq!(
      schema_for_dialect(TraceSchemaDialect::Sqlite),
      SQLITE_TRACE_SCHEMA
    );
  }
}
