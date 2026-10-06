-- AgentFlow Gateway: initial control-plane schema (v0.3.0 N8).
--
-- Six tables back the platform skeleton:
--   runs            — workflow / agent runs submitted via the gateway
--   steps           — per-step trace inside a run (node, agent, tool_call, ...)
--   events          — append-only event log used by SSE subscribers + replay
--   artifacts       — files / URLs produced by a run, surfaced to API clients
--   skill_installs  — local skill registry mirror (lets the gateway list skills)
--   mcp_sessions    — MCP transport sessions opened during a run
--
-- Schema is deliberately conservative: no exotic Postgres features beyond
-- JSONB and TIMESTAMPTZ, so it runs on Postgres 13+ without extensions.

CREATE TABLE IF NOT EXISTS runs (
  id           UUID PRIMARY KEY,
  workflow     TEXT NOT NULL,
  status       TEXT NOT NULL,
  started_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  finished_at  TIMESTAMPTZ,
  run_dir      TEXT,
  tenant_id    TEXT NOT NULL DEFAULT 'default',
  error        TEXT
);

CREATE INDEX IF NOT EXISTS runs_tenant_started_idx
  ON runs (tenant_id, started_at DESC);

CREATE INDEX IF NOT EXISTS runs_status_idx
  ON runs (status)
  WHERE status IN ('queued', 'running');

CREATE TABLE IF NOT EXISTS steps (
  run_id       UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  seq          INTEGER NOT NULL,
  node_id      TEXT NOT NULL,
  kind         TEXT NOT NULL,
  status       TEXT NOT NULL,
  started_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  duration_ms  BIGINT,
  payload      JSONB,
  PRIMARY KEY (run_id, seq)
);

CREATE TABLE IF NOT EXISTS events (
  run_id   UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  seq      BIGINT NOT NULL,
  kind     TEXT NOT NULL,
  payload  JSONB NOT NULL,
  ts       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (run_id, seq)
);

CREATE INDEX IF NOT EXISTS events_run_ts_idx
  ON events (run_id, ts);

CREATE TABLE IF NOT EXISTS artifacts (
  id           UUID PRIMARY KEY,
  run_id       UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  node_id      TEXT NOT NULL,
  name         TEXT NOT NULL,
  path_or_url  TEXT NOT NULL,
  mime_type    TEXT,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS artifacts_run_idx ON artifacts (run_id);

CREATE TABLE IF NOT EXISTS skill_installs (
  name          TEXT NOT NULL,
  version       TEXT NOT NULL,
  source        TEXT NOT NULL,
  installed_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  checksum      TEXT,
  PRIMARY KEY (name, version)
);

CREATE TABLE IF NOT EXISTS mcp_sessions (
  id          UUID PRIMARY KEY,
  server      TEXT NOT NULL,
  started_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  ended_at    TIMESTAMPTZ,
  tool_calls  INTEGER NOT NULL DEFAULT 0,
  metadata    JSONB
);

CREATE INDEX IF NOT EXISTS mcp_sessions_server_idx
  ON mcp_sessions (server, started_at DESC);
