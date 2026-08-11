-- AgentFlow Gateway: cross-replica run admission (W4.2f).
--
-- `RunAdmissionRegistry` (agentflow-server) is a per-process in-memory
-- semaphore (concurrency) + fixed-window counter (rate) checked before
-- `POST /v1/runs` creates a `runs` row. With N gateway replicas, each
-- replica enforces the configured limit independently -- the effective
-- cluster-wide limit is silently multiplied by N, a live correctness
-- bug the instant `replicaCount > 1` (guarded against today only by
-- the Helm `allowMultiReplica` gate added in W4.2a).
--
-- This table backs the rate-limit half of `RunRepo::create_if_admitted`:
-- one row per tenant, reset-or-incremented atomically via
-- `INSERT ... ON CONFLICT (tenant_id) DO UPDATE ...`. The concurrency
-- half needs no new table -- it derives from `COUNT(*) FROM runs WHERE
-- tenant_id = $1 AND status IN ('queued', 'running')`, the `runs`
-- table's own authoritative status column, so a crashed replica's
-- stuck-`running` row can't permanently leak a held admission slot: the
-- count self-heals the moment that row reaches a terminal status
-- (whenever the cleanup sweep or an operator flips it).
--
-- Both checks run inside one transaction serialized per tenant via
-- `pg_advisory_xact_lock(hashtext('run_admission:' || tenant_id))`, so
-- concurrent admission attempts for the same tenant across every
-- replica can't both observe "under the limit" and both admit.
CREATE TABLE IF NOT EXISTS tenant_admission_windows (
  tenant_id          TEXT PRIMARY KEY,
  window_started_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  window_count       INTEGER NOT NULL DEFAULT 0
);
