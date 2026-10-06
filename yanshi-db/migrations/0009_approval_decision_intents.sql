-- AgentFlow Gateway: cross-replica approval decision intent (W4.2e).
--
-- `PendingApprovalRegistry` (agentflow-server) parks a `oneshot::Sender`
-- keyed by `(session_id, request_id)` -- pinned to whichever process's
-- executor task is blocked awaiting the decision. A `POST .../approvals/
-- {request_id}` request landing on a *different* gateway replica than the
-- one holding the entry finds nothing to resolve, even though the
-- approval genuinely exists.
--
-- `session_id` here is deliberately just TEXT, not a foreign key -- the
-- registry (and this table) is shared between two callers with different
-- ID namespaces: real `harness_sessions.id` values (harness-session
-- approvals) and `runs.id` values formatted as strings (skill-run
-- approvals, W4.1b). Both are UUID text either way, so no FK is added.
--
-- Mirrors `run_cancellation_intents`'s mechanism exactly: the deciding
-- replica writes this row (`ON CONFLICT DO NOTHING` -- first decision
-- wins) and fires a Postgres NOTIFY (`agentflow_approval_decisions`)
-- that every replica's listener turns into a local
-- `PendingApprovalRegistry::decide(...)` call -- a harmless no-op on
-- every replica except the one that actually parked the oneshot.
CREATE TABLE IF NOT EXISTS approval_decision_intents (
  session_id  TEXT NOT NULL,
  request_id  TEXT NOT NULL,
  tenant_id   TEXT NOT NULL,
  decision    JSONB NOT NULL,
  decided_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (session_id, request_id)
);
