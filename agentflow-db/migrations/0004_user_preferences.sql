-- P6.4 Durable user preferences.
--
-- Per-tenant key / value store the Web UI uses to persist:
--   - theme (light / dark)
--   - default security profile in the create-run form
--   - operator event filter expression (per-run, but the global default
--     lives here)
--   - pagination size for the run list
--
-- The agent-side `PreferenceStore` trait from P4.7 lives in
-- `agentflow-memory` and is scoped to (tenant, user, skill). The table
-- here is a separate, simpler concept: UI / operator preferences
-- bound only to the active tenant. Adding a `user_id` column when
-- multi-user JWT lands is additive.
--
-- Stays on plain Postgres 13+ (JSONB + TIMESTAMPTZ).

CREATE TABLE IF NOT EXISTS user_preferences (
  tenant_id   TEXT NOT NULL DEFAULT 'default',
  key         TEXT NOT NULL,
  value       JSONB NOT NULL,
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (tenant_id, key)
);

CREATE INDEX IF NOT EXISTS user_preferences_tenant_idx
  ON user_preferences (tenant_id, updated_at DESC);
