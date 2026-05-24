# Audit: agentflow-db

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-db/
**Crate version**: 0.1.0 (workspace `version = "0.1.0"` in `agentflow-db/Cargo.toml:3`)
**Layer**: L4 (Operations / Productization, data-integrity-critical)
**Stability tier**: alpha-internal (no public stability promise; only `agentflow-server` consumes it; the crate carries `version = "0.1.0"` independently of the workspace 0.2.x train).

## Scope summary

Crate is a thin sqlx-backed persistence layer for the AgentFlow gateway. It exposes:
- `Database` connection-pool wrapper (primary + optional read-replica)
- `models.rs` row structs + `NewX` builders for every table
- `repo.rs` async-trait repository abstractions and `Pg*Repo` Postgres implementations
- `migrations/` 5 monotonically-numbered SQL files embedded via `sqlx::migrate!()`
- Integration tests in `tests/` gated by `AGENTFLOW_DATABASE_TEST_URL`

The crate is dependency-clean — no upward (L1/L2/L3) imports. The only AgentFlow consumer is `agentflow-server` (confirmed: 0 hits in any other workspace member).

Tenant-scoping enforcement was the focus of recent commits (`470893b feat(server): close P2.6 tenant/session boundary`, `55a5fa9 test(server): pin P2.6 tenant boundary on every :id-bound endpoint`). The current design layers tenant checks at the **server** route layer (every `:id`-bound endpoint loads the parent row and compares `tenant_id`), with the repo layer providing only the typed columns. This leaves three latent gaps inside the repo itself — see C1, M1, M2 below.

## Findings

### CRITICAL (data loss / tenant boundary breach)

- [C1] `SkillInstallRepo::list()` returns rows across **all** tenants — `agentflow-db/src/repo.rs:434-442`
  **What**: After migration `0003_tenant_id_columns.sql` repartitioned `skill_installs` with PK `(tenant_id, name, version)` and added a `tenant_id` column, the `list()` query was not updated:
  ```rust
  r#"SELECT name, version, source, installed_at, checksum, tenant_id
     FROM skill_installs ORDER BY name ASC, version ASC"#
  ```
  There is no `WHERE tenant_id = $1` clause and the trait signature itself takes no tenant (`async fn list(&self) -> Result<Vec<SkillInstall>, DbError>;`, `repo.rs:81`).
  **Why it matters**: The first caller that wires this up (any `/v1/skills` list endpoint on the gateway, mentioned in CLAUDE.md L4 as "platform skeleton") will silently leak the skill catalogue across tenant boundaries. Same anti-pattern that P2.6 closed for every other table. Today no server route imports `SkillInstallRepo`, so the leak is dormant — but it's a footgun primed to detonate the next time someone wires the skills marketplace to the server. The test at `tests/repositories.rs:293-347` (`skill_install_repo_upsert_replaces_on_conflict`) only ever uses `tenant_id = "default"` so it cannot catch the leak.
  **Fix**: Change the trait to `async fn list(&self, tenant_id: &str)` and add `WHERE tenant_id = $1` to the query. Add a new test that seeds two tenants and asserts isolation. Add a `CREATE INDEX skill_installs_tenant_idx ON skill_installs (tenant_id, name)` in a follow-up migration so the filtered query stays cheap when the catalogue grows.

### MAJOR

- [M1] `mcp_sessions` table has no `tenant_id` column — `agentflow-db/migrations/0001_initial_schema.sql:77-84`
  **What**: Unlike every other production table, `mcp_sessions` was never extended in `0003_tenant_id_columns.sql`. The `McpSessionRepo::open` / `::close` methods (`repo.rs:451-487`) accept no tenant either.
  **Why it matters**: The gateway is supposed to surface MCP transport sessions to operators per CLAUDE.md (L4 "platform skeleton"). When the route lands, the table cannot enforce row-level tenant isolation — every tenant sees every other tenant's MCP server URIs (which may contain credentials in `metadata` JSONB) and tool-call counts. Today the table is unused server-side so no live leak, but the schema is the long-lived artifact.
  **Fix**: Add `tenant_id TEXT NOT NULL DEFAULT 'default'` in a new migration, plus a composite `(tenant_id, started_at DESC)` index, plus `tenant_id` on both the model and `open`/`close` signatures.

- [M2] `EventRepo::list_after` and `HarnessEventRepo::list_after` ignore `tenant_id` — `repo.rs:345-364, 666-685`
  **What**: Both queries filter only by `(run_id, seq)` / `(session_id, seq)`. The `events` table got a `tenant_id` column + `events_tenant_run_idx (tenant_id, run_id, seq)` index in `0003_tenant_id_columns.sql:21-35` precisely so multi-tenant production can do row-level filtering, but the repo never uses the column. The server layer compensates by loading the parent run/session, checking `tenant_id` in Rust, then trusting the run_id (see `agentflow-server/src/events_stream.rs:221-238`, `harness.rs:561-571`).
  **Why it matters**: Defense-in-depth — every other gateway endpoint enforces tenant at both the route layer (404 on cross-tenant) and the DB layer (WHERE tenant_id). The events queries are the lone exception, so a future route that forgets to call `runs.get(id)` first would stream another tenant's events. The new index sits idle. The query also misses the index when `tenant_id` is hot in the planner stats.
  **Fix**: Add `tenant_id: &str` to the trait, add `AND tenant_id = $2` to both queries, and route every call site to forward the tenant it already has on hand. Backwards-compatible: also keep a non-tenant overload deprecated, or migrate call sites in one PR.

- [m3 → M3] `next_event_seq` server-side allocator over `events.list_after(..., 10_000)` can collide on huge runs — `agentflow-server/src/runs.rs:733-743`
  (Not strictly an `agentflow-db` bug, but the repo layer should offer the right primitive.)
  **What**: The server computes the next event seq by listing up to 10_000 events for the run and taking `max(seq) + 1`. If a run logs more than 10_000 events the cap silently truncates, returning a stale max → the next `INSERT INTO events` collides on the `(run_id, seq)` PK and the route 5xxs. The repo already implements the analogous `HarnessEventRepo::max_seq` (`repo.rs:687-694`) using `SELECT MAX(seq)`; the workflow `EventRepo` is missing that helper.
  **Why it matters**: A long-running workflow that emits >10k events is plausible (Map node fan-out + per-iter steps × per-tool sub-events). The failure mode is data-corruption-shaped (PK collision = 500 on the very next event append) rather than data loss, but it strands the SSE stream.
  **Fix**: Add `async fn max_seq(&self, run_id: Uuid) -> Result<Option<i64>, DbError>` to `EventRepo`, implement with `SELECT MAX(seq) FROM events WHERE run_id = $1`, and switch the server to it. Mirrors the harness side exactly.

- [M4] Connection pool config exposes only `max_connections` + 3 s acquire timeout — `agentflow-db/src/database.rs:38-42, 67-71`
  **What**: `PgPoolOptions::new().max_connections(N).acquire_timeout(3s).connect(...)`. No `min_connections`, no `idle_timeout`, no `max_lifetime`, no `test_before_acquire`. The 3 s acquire timeout is also aggressive for a busy primary under pgbouncer.
  **Why it matters**: In production: (a) idle TCP sockets to RDS get reaped by NAT/cloud LB after 5-15 min, and without `test_before_acquire(true)` the first query after a quiet period fails with `ConnectionClosed`; (b) without `max_lifetime` pgbouncer + RDS rolling failover never recycle connections; (c) tight 3 s `acquire_timeout` means a transient primary pause turns into user-visible 5xx instead of brief latency. The doc-string at `database.rs:30` mentions read-replica routing but doesn't surface any of the operational knobs.
  **Fix**: Promote pool settings to a `PoolOptions { max_conns, min_conns, acquire_timeout, idle_timeout, max_lifetime, test_before_acquire }` struct, with documented production defaults (e.g., 5/30/30s/600s/1800s/true). Keep `connect(url, max_connections)` as the simple shim for tests.

- [M5] Migration `0003` does inline `UPDATE ... FROM` backfill without batching — `migrations/0003_tenant_id_columns.sql:27-32, 41-46`
  **What**: The migration runs `UPDATE events SET tenant_id = runs.tenant_id FROM runs WHERE events.run_id = runs.id AND events.tenant_id = 'default' AND runs.tenant_id <> 'default'` in a single statement. On a production database with millions of `events` rows this acquires a write lock for the duration and blocks all writers until commit. Same for `artifacts`.
  **Why it matters**: First production deploy of P2.6 on a busy gateway = multi-minute write stall and `acquire_timeout` 5xx storm (see M4). The migration uses `ADD COLUMN IF NOT EXISTS ... NOT NULL DEFAULT 'default'` first, which Postgres 11+ does as a metadata-only operation (good!), but the subsequent `UPDATE` is not online.
  **Fix**: Either (a) split the backfill into a separate, idempotent CLI step that batches in 10k-row chunks with `WHERE tenant_id = 'default' AND id IN (...)`, or (b) document explicitly that this migration must run in a maintenance window. Add a regression test that times the migration against a seeded 1M-row dataset (gated, opt-in).

### MINOR

- [m1] `reset_for_resume` deletes child events *and* updates the parent, but the parent UPDATE failing leaves the events already deleted — `agentflow-db/src/repo.rs:587-617`
  Currently both statements run inside a single `tx.begin()...tx.commit()` transaction, so the delete is rolled back on the UPDATE error. **No actual bug** — flagging because the inline comment ("we keep the session row — so the child rows are deleted explicitly here", lines 590-592) is somewhat misleading. Suggest tightening the comment to "the transaction guarantees we never observe a half-reset state" since the existing code already does the right thing. Verified by `harness_session_reset_for_resume_wipes_events` (`tests/repositories.rs:503-567`).

- [m2] `unwrap()/expect()` count in non-test code: **0**.
  Only three hits: `database.rs:147` and `repo.rs:883` are both `.expect("lazy pool")` inside `#[cfg(test)] mod tests`, and `database.rs:172` is `assert!(..., db.read_pool.as_ref().unwrap())` also inside `#[cfg(test)]`. Production code is clean. (Top-5 list below is provided for completeness even though all are test-only.)

- [m3] `TODO/FIXME/XXX/HACK` markers: **0** across `src/`, `migrations/`, and `tests/`. Clean.

- [m4] Doc-comment drift: `models.rs:1` says "six-table schema" and `repo.rs:1` says "the gateway's six tables" — there are now 9 production tables (`runs`, `steps`, `events`, `artifacts`, `skill_installs`, `mcp_sessions`, `harness_sessions`, `harness_session_events`, `user_preferences`). `migrations/0001_initial_schema.sql:3-9` also lists "Six tables" but it's accurate for that file; the umbrella docstrings in `lib.rs` / `models.rs` / `repo.rs` should be refreshed. CLAUDE.md L4 description also says "Eight-table schema" — also out of date by one.

- [m5] `PgRunRepo::list_filtered` issues two literally-different query strings depending on whether `status` is `Some`/`None` (`repo.rs:248-277`) with an inline comment saying "so the optimizer can pick the right index without needing to read a NULL parameter at runtime." That's a defensible micro-optimization but doubles the query-cache footprint. Consider a single query of the form `WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2)` — Postgres treats it as a constant predicate at plan time when the parameter is a literal NULL via prepared-statement plan-cache invalidation. Either way is fine; the comment as-is is the right answer if perf has been measured.

- [m6] `from_pool` / `from_pools` / `from_database` test (`repo.rs:887-951`) asserts `std::ptr::eq(&repos.runs.pool, &repos.runs.pool)` (line 893) which is tautologically true (same expression on both sides) and so cannot actually fail. The downstream assertions about replica/primary distinction at lines 932 are meaningful. Suggest dropping the tautology or rewriting it to compare against the input pool variable.

- [m7] `default_tenant_id()` (`models.rs:136-138`) returns a fresh `String::from("default")` on every deserialise; consider `Cow<'static, str>` or making the column `NOT NULL` in the model (the migration already makes it NOT NULL at the DB layer — `0003_tenant_id_columns.sql:23`). Today serde fills in `"default"` only for clients that omit the field, which is friendly for compat but encourages tenant ambiguity.

- [m8] `Repositories::from_pool` field-by-field clones via `pool.clone()` 8 times (`repo.rs:728-763`). `PgPool` is `Arc<...>` internally so this is cheap, but the boilerplate is real — a `#[derive(Clone)]` plus a constructor that builds one `Pg*Repo` at a time would be tidier. Optional cleanup.

- [m9] `mcp_sessions.metadata` is unbounded JSONB and `events.payload` likewise. There is no DB-level size cap, no application-level cap on serialised payload size, and no warning in the docstring about logging giant tool outputs. A noisy tool can wedge the events table; a follow-up `CHECK (octet_length(payload::text) < 1048576)` constraint plus a server-side serialiser guard would be defense-in-depth.

- [m10] `sqlx` is used in **runtime-checked** mode (`query_as::<_, T>(literal_sql)`) not `query!`/`query_as!` macros — see e.g. `repo.rs:172`. This is a deliberate choice (no live DB required at compile time, simpler CI), and the trade-off is documented implicitly by the absence of `sqlx-cli`/`DATABASE_URL` in the build. Flag for awareness: an SQL typo only fails at integration-test time, not compile time. The integration tests in `tests/repositories.rs` cover the critical paths so the risk is bounded; mentioning it explicitly in `lib.rs` would prevent the next contributor from wondering.

- [m11] `chrono` is used (not `time`); `uuid` features `v4 + serde` — both are mainstream choices consistent with the workspace. No issues.

- [m12] Tests are gated by `AGENTFLOW_DATABASE_TEST_URL` and intentionally do **not** TRUNCATE between tests (see `tests/repositories.rs:30-36` comment block). Each test uses a UUID-suffixed tenant/skill name. This is a sound trade-off (parallel tests can't TRUNCATE under each other) but it means a long-lived CI database accumulates orphan rows — flagged in the comment as a follow-up. No `testcontainers` integration today; tests rely on an externally-provisioned Postgres.

### POSITIVE OBSERVATIONS

- **Zero `unwrap()/expect()` in non-test code.** Repository errors propagate via `?` into a typed `DbError` with explicit `NotFound { entity_type, id }` distinguishing missing rows from connection failures (`error.rs:10-14`). `RunRepo::update_status` / `McpSessionRepo::close` / both Harness reset methods all check `result.rows_affected() == 0` and return the typed `NotFound`, so the route layer can map cleanly to HTTP 404 vs 5xx (verified at `repo.rs:229-235, 479-485, 609-614, 633-638`).
- **All queries are parameterised** — zero `format!`-based SQL across `src/`. Search for `format!.*FROM|SELECT|INSERT|UPDATE|WHERE` returns zero hits in `agentflow-db/src/`. Strong baseline.
- **Migration filenames are monotonically numbered, no gaps**: `0001` → `0005`. Each migration is idempotent (`IF NOT EXISTS` everywhere it matters, `DROP CONSTRAINT IF EXISTS` for the PK swap). `sqlx::migrate!()` tracks state in `_sqlx_migrations` per `database.rs:117`.
- **Indexes match the dominant query patterns**: `runs_tenant_started_idx (tenant_id, started_at DESC)` for the list endpoint, `events_tenant_run_idx (tenant_id, run_id, seq)` for tenant-scoped backfill, `runs_status_idx (status) WHERE status IN ('queued', 'running')` as a partial for the scheduler poll. `harness_sessions_status_idx (status) WHERE status = 'running'` similarly. Index discipline is good.
- **Foreign keys all use explicit `ON DELETE CASCADE`** (`steps`, `events`, `artifacts` → `runs`; `harness_session_events` → `harness_sessions`). Run retention sweep can `DELETE FROM runs` and the children go with it, atomically.
- **Read-replica routing (P10.15.2)** is in place via `Database::read_pool()` and every `Pg*Repo` carries both `pool` (writes) and `read_pool` (reads). `Repositories::from_database` threads the replica through correctly when configured (`repo.rs:771-773`). Doc-string at `database.rs:14-24` is honest about replication lag.
- **`reset_for_resume` is transactional** (`repo.rs:587-617`) — `DELETE FROM harness_session_events` and `UPDATE harness_sessions SET status='running'` happen in a single `tx.begin()/tx.commit()`, so a concurrent reader never sees a half-reset row.
- **`upsert_many` for user preferences is transactional** (`repo.rs:824-849`) — bulk-write atomicity guaranteed; the route docstring at `repo.rs:784-786` calls this out explicitly.
- **Integration tests cover the closed surface**: every repo has CRUD test coverage in `tests/repositories.rs` (Run, Step, Event, Artifact, SkillInstall, McpSession, HarnessSession, HarnessEvent — UserPreference is the only gap), tenant isolation is asserted at `tests/repositories.rs:171-198`, and `reset_for_resume` / `reset_for_append_resume` both have dedicated tests (`tests/repositories.rs:502-629`). The cleanup-via-uniqueness pattern (UUID-suffixed tenant strings) is documented in the `fresh_db()` comment.
- **L4 boundary intact**: zero imports from `agentflow-core` / `agentflow-llm` / `agentflow-agents` / `agentflow-skills`. The only external workspace footprint is consumers depending on this crate, not the reverse.

## Metrics

- Source files: **4** (`lib.rs`, `error.rs`, `database.rs`, `models.rs`, `repo.rs` — 5 counting `lib.rs`)
- Lines of code: **1509** in `src/` (175 + 17 + 18 + 347 + 952), **255** in `migrations/`, **682** in `tests/` → **2446 total**
- Migrations: **5**
  - `0001_initial_schema.sql` — runs, steps, events, artifacts, skill_installs, mcp_sessions
  - `0002_harness_sessions.sql` — harness_sessions, harness_session_events
  - `0003_tenant_id_columns.sql` — backfill tenant_id on events/artifacts/skill_installs, re-PK skill_installs
  - `0004_user_preferences.sql` — user_preferences
  - `0005_run_retention_overrides.sql` — adds `events_retention_days` / `artifacts_retention_days` to `runs`
- Tables: **9** (runs, steps, events, artifacts, skill_installs, mcp_sessions, harness_sessions, harness_session_events, user_preferences). CLAUDE.md L4 says "Eight-table schema" — **doc drift, off by one** (missing `user_preferences`).
- Repos: **9 traits + 9 Pg* impls** (RunRepo, StepRepo, EventRepo, ArtifactRepo, SkillInstallRepo, McpSessionRepo, HarnessSessionRepo, HarnessEventRepo, UserPreferenceRepo). CLAUDE.md L4 lists 8 — same off-by-one as the tables.
- Test files: **0 unit + 2 integration** (`tests/migrations.rs`, `tests/repositories.rs`), plus inline `#[cfg(test)] mod tests` blocks in `database.rs` and `repo.rs` for pool-routing logic (2 unit tests in `database.rs`, 4 unit tests in `repo.rs`, 2 round-trip tests in `models.rs`). 13 integration tests in `tests/repositories.rs`, 1 in `tests/migrations.rs`. **Gated** on `AGENTFLOW_DATABASE_TEST_URL` — `cargo test --workspace` skips them by default.
- `unwrap()/expect()` in non-test code: **0**. All three hits are in `#[cfg(test)]` blocks:
  - `src/database.rs:147` — `.expect("lazy pool")` (test helper)
  - `src/database.rs:172` — `.unwrap()` (test assertion)
  - `src/repo.rs:883` — `.expect("lazy pool")` (test helper)
- TODO/FIXME: **0**.
- Public items missing rustdoc: estimated **~10-15%**. Most public APIs have `///` doc comments (`repo.rs` has 182 doc-comment lines across 952 LOC, dense coverage on trait methods and `Pg*Repo` structs). Notable gaps: `RunStatus::as_str` / `parse` (no doc), `HarnessSessionStatus::as_str` (no doc), several `NewX` builder structs (`NewStep`, `NewArtifact`, `NewHarnessSessionEvent`, `NewUserPreference`) have no docstring on the struct itself even when their `New*` siblings do.

## Recommendations (prioritized)

1. **Fix C1** — Update `SkillInstallRepo::list()` to take a `tenant_id` and filter on it; add a tenant-isolation test. Highest priority because it's a primed footgun the moment any server route subscribes.
2. **Fix M2** — Push `tenant_id` filter into the two `list_after` queries so the events index actually gets used and the DB enforces defense-in-depth instead of trusting the route layer alone.
3. **Fix M1** — Add `tenant_id` to `mcp_sessions` in a new migration before the gateway ever surfaces this table. Easier to do now than after rows accumulate.
4. **Add `EventRepo::max_seq`** (M3) and switch `agentflow-server/src/runs.rs:733-743` to use it, eliminating the 10k-event truncation footgun.
5. **Document + parameterise pool options** (M4) — at minimum set `test_before_acquire(true)` and `max_lifetime(30 min)` defaults to survive cloud-LB connection reaping.
6. **Batch the M5 backfill** or document the maintenance-window requirement before the first production multi-tenant deploy hits a busy events table.
7. **Refresh stale docstrings** (m4) — `lib.rs`, `models.rs:1`, `repo.rs:1`, and CLAUDE.md L4 all undercount the tables/repos.
8. **Add unit test for `UserPreferenceRepo`** — the only repo without integration coverage in `tests/repositories.rs`.
9. **Optional**: pivot tests to `testcontainers` so they self-provision Postgres instead of requiring `AGENTFLOW_DATABASE_TEST_URL`; tracked as follow-up only because it changes CI shape.
10. **Optional**: add a `CHECK (octet_length(payload::text) < N)` constraint on `events.payload` / `harness_session_events.payload` / `mcp_sessions.metadata` to cap pathological writes (m9).

End of report.
