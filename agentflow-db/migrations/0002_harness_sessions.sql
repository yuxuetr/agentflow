-- P-H.5 Harness Mode control-plane schema (v0.4.0).
--
-- Two new tables sit alongside the workflow tables from `0001`:
--   harness_sessions       — agent-native sessions submitted via /v1/harness/sessions
--   harness_session_events — append-only event log; SSE subscribers use it
--                            for backfill + reconnect with `?after_seq=`
--
-- Sessions are intentionally kept separate from `runs` (rather than adding
-- a `kind` column) because their lifecycle fields diverge meaningfully:
-- harness sessions carry a workspace root, security profile, runtime kind,
-- model handle, and optional skill, while workflow runs carry the inline
-- workflow text. Two narrow tables keep each model strongly-typed and
-- avoid sentinel/null columns.
--
-- Stays on plain Postgres 13+ (JSONB + TIMESTAMPTZ, no extensions).

CREATE TABLE IF NOT EXISTS harness_sessions (
  id              UUID PRIMARY KEY,
  tenant_id       TEXT NOT NULL DEFAULT 'default',
  status          TEXT NOT NULL,
  user_input      TEXT NOT NULL,
  workspace_root  TEXT NOT NULL,
  profile         TEXT NOT NULL,
  runtime_kind    TEXT NOT NULL,
  model           TEXT NOT NULL,
  skill_name      TEXT,
  started_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  finished_at     TIMESTAMPTZ,
  final_answer    TEXT,
  error           TEXT
);

CREATE INDEX IF NOT EXISTS harness_sessions_tenant_started_idx
  ON harness_sessions (tenant_id, started_at DESC);

CREATE INDEX IF NOT EXISTS harness_sessions_status_idx
  ON harness_sessions (status)
  WHERE status = 'running';

CREATE TABLE IF NOT EXISTS harness_session_events (
  session_id  UUID NOT NULL REFERENCES harness_sessions(id) ON DELETE CASCADE,
  seq         BIGINT NOT NULL,
  kind        TEXT NOT NULL,
  payload     JSONB NOT NULL,
  ts          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (session_id, seq)
);

CREATE INDEX IF NOT EXISTS harness_session_events_session_ts_idx
  ON harness_session_events (session_id, ts);
