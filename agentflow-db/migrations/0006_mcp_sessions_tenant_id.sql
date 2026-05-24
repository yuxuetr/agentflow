-- Q1.5.2 — tenant-id column for `mcp_sessions`.
--
-- Migration 0003 added `tenant_id` to events / artifacts / skill_installs
-- but skipped `mcp_sessions`, leaving the repo with no way to enforce
-- row-level tenant isolation. This migration closes the gap.
--
-- Backfill strategy: no FK to lean on (mcp_sessions has no `run_id`
-- linkage today), so existing rows default to `'default'` — same
-- treatment as 0003 gave skill_installs. Production multi-tenant
-- deployments should not have historical mcp_sessions rows worth
-- preserving anyway since the table is short-lived (one row per
-- live MCP connection during a run).
--
-- A composite index on (tenant_id, started_at DESC) so a future list
-- endpoint can serve newest-first within a tenant without a table scan.

ALTER TABLE mcp_sessions
  ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';

CREATE INDEX IF NOT EXISTS mcp_sessions_tenant_started_idx
  ON mcp_sessions (tenant_id, started_at DESC);
