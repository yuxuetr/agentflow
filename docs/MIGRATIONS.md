# AgentFlow Database Migrations — Operator Playbook

This document covers operational guidance for applying the `agentflow-db`
migrations against an existing production-sized Postgres deployment.

For fresh installs (empty database), every migration applies in
sub-second time; you can skip directly to **§ Standard upgrade**.

The migrations themselves live in `agentflow-db/migrations/`. They are
applied automatically by `Database::connect_and_migrate(...)` at server
boot and tracked in the `_sqlx_migrations` table.

---

## Standard upgrade (small to medium dataset)

For databases where the largest first-class tables (`events`,
`artifacts`, `harness_session_events`) are under ~10 million rows on
warm storage:

1. Take a backup. Migrations are idempotent, but `pg_dump` is cheap.
2. Bring the server down (or drain new traffic via `terminationGracePeriodSeconds`).
3. Start the new server binary. `connect_and_migrate` will apply
   pending migrations in order against the primary pool.
4. Validate `SELECT MAX(version) FROM _sqlx_migrations` matches the
   latest `.sql` file in `agentflow-db/migrations/`.

Total downtime: seconds to a few minutes.

---

## Q3.11.3 — Migration `0003_tenant_id_columns.sql` on large `events` / `artifacts`

`0003` introduces tenant-scoping for `events`, `artifacts`, and
`skill_installs`. The backfill step uses an unbatched
`UPDATE events SET tenant_id = runs.tenant_id FROM runs WHERE ...`.
On a small table this takes milliseconds; on a multi-hundred-million-row
`events` table the single statement acquires an exclusive lock and
holds it for the duration of the scan, blocking every concurrent
read and write against the table.

If your `events` row count exceeds ~50 million OR you cannot tolerate
the implied write-pause, apply the migration **out of band** before
the application's auto-migrator runs:

### Step 1 — Add the column with the default (fast, takes a brief AccessExclusive lock)

```sql
ALTER TABLE events
  ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';
```

This is a metadata-only operation on Postgres 11+ — every existing
row inherits the `'default'` literal without rewrite. Takes a few
milliseconds even on billion-row tables.

### Step 2 — Backfill in batches (long-running, no exclusive lock)

```sql
-- Tune BATCH_SIZE to your storage; 10_000 is conservative for AWS RDS
-- gp3, 100_000 fine for io1/io2.
DO $$
DECLARE
  rows_updated INT;
BEGIN
  LOOP
    WITH batch AS (
      SELECT events.run_id, events.seq
        FROM events
        JOIN runs ON events.run_id = runs.id
       WHERE events.tenant_id = 'default'
         AND runs.tenant_id <> 'default'
       LIMIT 10000
       FOR UPDATE OF events SKIP LOCKED
    )
    UPDATE events
       SET tenant_id = runs.tenant_id
      FROM batch
      JOIN runs ON runs.id = batch.run_id
     WHERE events.run_id = batch.run_id
       AND events.seq = batch.seq;
    GET DIAGNOSTICS rows_updated = ROW_COUNT;
    EXIT WHEN rows_updated = 0;
    PERFORM pg_sleep(0.05);  -- yield, keep replication lag bounded
  END LOOP;
END $$;
```

Run this in a `psql` session OUTSIDE the application. Each iteration
takes an exclusive lock only on the 10 000 rows in `batch` (via
`FOR UPDATE OF ... SKIP LOCKED`), so concurrent reads / writes
continue unaffected. The whole pass typically runs at ~50 k rows/sec
on RDS db.m5.large.

Repeat the same shape for `artifacts`:

```sql
DO $$
DECLARE rows_updated INT;
BEGIN
  LOOP
    WITH batch AS (
      SELECT id FROM artifacts
       WHERE tenant_id = 'default'
         AND run_id IN (SELECT id FROM runs WHERE tenant_id <> 'default')
       LIMIT 10000
       FOR UPDATE SKIP LOCKED
    )
    UPDATE artifacts SET tenant_id = runs.tenant_id
      FROM batch
      JOIN runs ON runs.id = artifacts.run_id
     WHERE artifacts.id = batch.id;
    GET DIAGNOSTICS rows_updated = ROW_COUNT;
    EXIT WHEN rows_updated = 0;
    PERFORM pg_sleep(0.05);
  END LOOP;
END $$;
```

### Step 3 — Create the index out of band

```sql
CREATE INDEX CONCURRENTLY IF NOT EXISTS events_tenant_run_idx
  ON events (tenant_id, run_id, seq);
CREATE INDEX CONCURRENTLY IF NOT EXISTS artifacts_tenant_run_idx
  ON artifacts (tenant_id, run_id);
```

`CONCURRENTLY` avoids the table-level write lock the inline
`CREATE INDEX` in the migration would have taken. The migration file
uses `CREATE INDEX IF NOT EXISTS` which is a no-op once the
concurrent build has finished.

### Step 4 — Apply the migration normally

After the manual backfill + index build is complete, start the new
server binary. The migration's `UPDATE ... WHERE events.tenant_id =
'default' AND runs.tenant_id <> 'default'` clause matches zero rows
on the second pass (everything is already backfilled), so the
auto-migrator finishes in milliseconds. The `CREATE INDEX IF NOT
EXISTS` is also a no-op.

### Step 5 — Validate

```sql
-- Every events row whose owning run is non-default-tenant must now
-- carry that tenant.
SELECT events.tenant_id, runs.tenant_id, COUNT(*)
  FROM events JOIN runs ON events.run_id = runs.id
 WHERE events.tenant_id <> runs.tenant_id
 GROUP BY 1, 2;
-- Expect: 0 rows.
```

---

## Future migration changes

If a NEW migration needs a large backfill, ship it as a no-op
backfill in the `.sql` file and document the manual batched
playbook here, the same shape this Q3.11.3 entry uses. Modifying
**existing** migrations is a breaking change — `sqlx::migrate!`
checksums every file at compile time and refuses to run when a
deployed migration's hash drifts.

For schema-only additions (new column, new index), use the
`CONCURRENTLY` variants when the table is large.

---

## Rollback

`agentflow-db` migrations are **forward-only**. There are no `.down.sql`
files. Rolling back a migration requires restoring from backup or
applying compensating SQL manually. The combination of column
backfills and index builds in `0003` is particularly hard to undo
cleanly — keep `pg_dump` snapshots between application upgrades.
