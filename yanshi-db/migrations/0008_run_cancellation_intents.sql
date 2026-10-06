-- AgentFlow Gateway: cross-replica run cancellation intent (W4.2d).
--
-- `RunCancellationRegistry` (agentflow-server) holds a `FlowCancellationToken`
-- + `AbortHandle` per run_id -- both pinned to whichever process's tokio
-- runtime spawned the executor task. A `POST /v1/runs/{id}:cancel` request
-- landing on a *different* gateway replica than the one running the task
-- has nothing local to cancel; before this table existed that request
-- silently no-op'd while still reporting `cancelled: true` to the caller.
--
-- This table durably records "someone asked to cancel this run" so every
-- replica's `cancel_run` handler can write the intent (`ON CONFLICT DO
-- NOTHING` -- first cancel request wins, later ones are redundant) and
-- fire a Postgres NOTIFY (`agentflow_cancellations`, payload is just the
-- run_id) that every replica's listener picks up and turns into a local
-- `RunCancellationRegistry::cancel(run_id)` call -- a harmless no-op on
-- every replica except the one that actually holds the entry.
CREATE TABLE IF NOT EXISTS run_cancellation_intents (
  run_id       UUID PRIMARY KEY REFERENCES runs(id) ON DELETE CASCADE,
  tenant_id    TEXT NOT NULL,
  requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
