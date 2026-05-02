//! Integration tests for the embedded sqlx migrations.
//!
//! These tests are gated by the `AGENTFLOW_DATABASE_TEST_URL` env var so that
//! `cargo test --workspace` stays hermetic (no Docker / Postgres required).
//! To run locally:
//!
//! ```bash
//! AGENTFLOW_DATABASE_TEST_URL=postgres://postgres:postgres@localhost:5432/agentflow_test \
//!   cargo test -p agentflow-db --test migrations
//! ```

use agentflow_db::Database;

/// Skip the test when no live Postgres is configured.
fn live_database_url() -> Option<String> {
  std::env::var("AGENTFLOW_DATABASE_TEST_URL").ok()
}

#[tokio::test]
async fn migrations_apply_idempotently() {
  let Some(url) = live_database_url() else {
    eprintln!("skipping migrations_apply_idempotently — set AGENTFLOW_DATABASE_TEST_URL to run");
    return;
  };

  let db = Database::connect_and_migrate(&url, 4)
    .await
    .expect("first migration run should succeed");

  // Re-running should be a no-op (idempotency is the contract).
  db.run_migrations()
    .await
    .expect("second migration run should be a no-op");

  // Smoke check: all six tables exist.
  for table in [
    "runs",
    "steps",
    "events",
    "artifacts",
    "skill_installs",
    "mcp_sessions",
  ] {
    let row: (bool,) = sqlx::query_as(
      "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1)",
    )
    .bind(table)
    .fetch_one(&db.pool)
    .await
    .unwrap_or_else(|e| panic!("query for table {table} failed: {e}"));
    assert!(row.0, "table `{table}` was not created by migrations");
  }
}
