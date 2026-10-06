-- P2.6 tenant/session boundary.
--
-- The `runs` and `harness_sessions` tables already carry `tenant_id`
-- (added in 0001 / 0002). This migration extends the same scoping to
-- the three remaining first-class tables that the gateway exposes:
-- `events`, `artifacts`, and `skill_installs`.
--
-- Why all three: every row a downstream client (CLI, Web UI, server
-- API) reads or lists is now tenant-scoped, so a single-tenant
-- "default" deployment stays zero-config while a multi-tenant
-- production deployment can enforce row-level isolation with a single
-- `WHERE tenant_id = $1` clause.
--
-- Events + artifacts back-fill from their owning `runs` row via the
-- existing FK, so historical data continues to surface under the
-- correct tenant after migration. `skill_installs` has no FK to lean
-- on, so existing rows default to the canonical `'default'` tenant.
--
-- Stays on Postgres 13+ (no extensions).

-- ── events ────────────────────────────────────────────────────────────
ALTER TABLE events
  ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';

-- Back-fill from the owning run so historical rows match the
-- tenant they were created under.
UPDATE events
  SET tenant_id = runs.tenant_id
  FROM runs
  WHERE events.run_id = runs.id
    AND events.tenant_id = 'default'
    AND runs.tenant_id <> 'default';

CREATE INDEX IF NOT EXISTS events_tenant_run_idx
  ON events (tenant_id, run_id, seq);

-- ── artifacts ─────────────────────────────────────────────────────────
ALTER TABLE artifacts
  ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';

UPDATE artifacts
  SET tenant_id = runs.tenant_id
  FROM runs
  WHERE artifacts.run_id = runs.id
    AND artifacts.tenant_id = 'default'
    AND runs.tenant_id <> 'default';

CREATE INDEX IF NOT EXISTS artifacts_tenant_run_idx
  ON artifacts (tenant_id, run_id);

-- ── skill_installs ────────────────────────────────────────────────────
ALTER TABLE skill_installs
  ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';

-- Primary key was (name, version). Multi-tenant installs need
-- (tenant_id, name, version) so two tenants can install the same
-- skill at the same version independently. Dropping a PRIMARY KEY
-- requires the constraint name; Postgres auto-names it
-- `<table>_pkey`.
ALTER TABLE skill_installs
  DROP CONSTRAINT IF EXISTS skill_installs_pkey;
ALTER TABLE skill_installs
  ADD CONSTRAINT skill_installs_pkey PRIMARY KEY (tenant_id, name, version);
