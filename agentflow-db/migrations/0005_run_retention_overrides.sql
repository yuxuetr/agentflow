-- AgentFlow Gateway: per-run retention overrides (P10.14.1).
--
-- Today retention is per-tenant + per-profile (see
-- `agentflow-server/src/cleanup.rs::CleanupConfig::for_profile`).
-- Per-run override (P2.2 deferred) lets a submitter ask the cleanup
-- sweep to keep events and/or artifacts of a specific critical run
-- longer than the global default. The override only ever *extends*
-- the retention window; it cannot shorten it (operators may have
-- compliance reasons to keep at least the global minimum).
--
-- Semantics enforced by the cleanup SQL:
--
--   effective_events_days  = GREATEST(global, COALESCE(override, 0))
--   effective_artifacts_days = GREATEST(global, COALESCE(override, 0))
--   effective_runs_days    = GREATEST(global, override_events,
--                                      override_artifacts, 0)
--
-- The third term pins the run row itself for the same window so the
-- `ON DELETE CASCADE` from `runs` doesn't pull events/artifacts out
-- from under the override.

ALTER TABLE runs
  ADD COLUMN IF NOT EXISTS events_retention_days INTEGER,
  ADD COLUMN IF NOT EXISTS artifacts_retention_days INTEGER;

-- Both columns default to NULL (no override); the cleanup sweep
-- treats NULL as 0 in the GREATEST(...) so the global retention
-- still applies unchanged.
