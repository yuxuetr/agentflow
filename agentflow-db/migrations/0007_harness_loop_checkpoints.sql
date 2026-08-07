-- V2.3: durable, server-side agent-loop checkpoint storage.
--
-- Distinct from `harness_session_events` (an append-only audit log): this
-- table holds exactly one row per session — only the *latest* loop
-- position matters for resume, the same "single-file-per-session,
-- overwrite in place" posture `agentflow-agents::FileLoopCheckpointer`
-- (the CLI's checkpointer) already uses. `DbLoopCheckpointer` is the
-- server's implementation of the same `AgentLoopCheckpointer` contract,
-- chosen over reusing the CLI's file-based one because the server has no
-- existing local-disk convention and a deeply established
-- Postgres-everything pattern for everything else in this schema.
--
-- `payload` carries the whole serialized `AgentLoopCheckpoint` (steps,
-- events, counters, pending_question, etc.) — see
-- `agentflow_agent_spi::checkpoint::AgentLoopCheckpoint`. `schema_version`
-- is duplicated out of the payload into its own column so a future
-- migration could filter/backfill by version without deserializing every
-- row's JSONB.

CREATE TABLE IF NOT EXISTS harness_loop_checkpoints (
  session_id      UUID PRIMARY KEY REFERENCES harness_sessions(id) ON DELETE CASCADE,
  schema_version  INT NOT NULL,
  payload         JSONB NOT NULL,
  updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- V2.3: the pending question a session is paused on, if any — read
-- directly by `GET /v1/harness/sessions/{id}/interrupt` without needing
-- to deserialize the checkpoint payload. `pending_question_step_index`
-- has no meaning when `pending_question` is NULL.
ALTER TABLE harness_sessions
  ADD COLUMN IF NOT EXISTS pending_question TEXT,
  ADD COLUMN IF NOT EXISTS pending_question_step_index BIGINT;
