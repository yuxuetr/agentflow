use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::info;

use crate::error::DbError;

/// Database connection manager.
///
/// The `pool` field is the **primary** (write-capable) pool. The
/// optional `read_pool` (P10.15.2) is a separate connection pool
/// pointed at a read replica; when present, the repository layer
/// routes `get_*` / `list_*` queries to it so a read-heavy gateway
/// can scale beyond the primary's connection budget. When `None`,
/// reads fall back to the primary — that's the existing behavior
/// and the default for single-node deployments.
///
/// Replication-lag caveat: writes go to `pool`, reads to
/// `read_pool` if set. A request that writes then immediately
/// reads (within a single round trip) may observe the prior
/// state because the replica hasn't caught up. The cleanup
/// sweep, run-row creation, and harness session creation all
/// use the primary for both read and write, so this only
/// affects HTTP clients that submit then re-query in the same
/// breath.
#[derive(Clone, Debug)]
pub struct Database {
  pub pool: PgPool,
  /// Optional read replica pool. `None` = reads use `pool`.
  pub read_pool: Option<PgPool>,
}

impl Database {
  /// Initialize a new PostgreSQL connection pool. Migrations are NOT applied —
  /// callers that own the schema should use [`Self::connect_and_migrate`].
  pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, DbError> {
    info!("Connecting to database...");

    let pool = PgPoolOptions::new()
      .max_connections(max_connections)
      .acquire_timeout(Duration::from_secs(3))
      .connect(database_url)
      .await?;

    info!("Successfully connected to database.");

    Ok(Self {
      pool,
      read_pool: None,
    })
  }

  /// Connect to both a primary and a read-replica URL. The primary
  /// is used for writes + migrations; the replica is used for
  /// `get_*` / `list_*` repository methods. P10.15.2.
  ///
  /// `replica_max_connections` controls the replica pool size
  /// independently of the primary — read-heavy gateways typically
  /// want this larger than the primary's write-cap.
  pub async fn connect_with_replica(
    database_url: &str,
    read_database_url: &str,
    max_connections: u32,
    replica_max_connections: u32,
  ) -> Result<Self, DbError> {
    let primary = Self::connect(database_url, max_connections).await?;
    info!("Connecting to read replica...");
    let read_pool = PgPoolOptions::new()
      .max_connections(replica_max_connections)
      .acquire_timeout(Duration::from_secs(3))
      .connect(read_database_url)
      .await?;
    info!("Successfully connected to read replica.");
    Ok(Self {
      pool: primary.pool,
      read_pool: Some(read_pool),
    })
  }

  /// Connect and run all embedded migrations from `agentflow-db/migrations/`.
  ///
  /// `sqlx::migrate!()` embeds the SQL files at compile time, so binaries
  /// shipped without source still apply the schema correctly. Migration is
  /// idempotent and tracked via the `_sqlx_migrations` table.
  pub async fn connect_and_migrate(
    database_url: &str,
    max_connections: u32,
  ) -> Result<Self, DbError> {
    let db = Self::connect(database_url, max_connections).await?;
    db.run_migrations().await?;
    Ok(db)
  }

  /// Connect to primary + replica and run migrations against the primary.
  /// P10.15.2.
  pub async fn connect_and_migrate_with_replica(
    database_url: &str,
    read_database_url: &str,
    max_connections: u32,
    replica_max_connections: u32,
  ) -> Result<Self, DbError> {
    let db = Self::connect_with_replica(
      database_url,
      read_database_url,
      max_connections,
      replica_max_connections,
    )
    .await?;
    db.run_migrations().await?;
    Ok(db)
  }

  /// Apply all pending migrations against the **primary** pool.
  /// Replicas catch up via Postgres streaming replication — we never
  /// run DDL against the replica directly.
  pub async fn run_migrations(&self) -> Result<(), DbError> {
    info!("Running database migrations...");
    sqlx::migrate!("./migrations")
      .run(&self.pool)
      .await
      .map_err(|e| DbError::MigrationError {
        message: e.to_string(),
      })?;
    info!("Database migrations applied.");
    Ok(())
  }

  /// Pool to route read queries to. Returns the replica when
  /// configured, otherwise the primary. Repository layer uses this
  /// for every `get_*` / `list_*` method.
  pub fn read_pool(&self) -> &PgPool {
    self.read_pool.as_ref().unwrap_or(&self.pool)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use sqlx::postgres::PgPoolOptions;

  fn lazy_pool() -> PgPool {
    // `connect_lazy` doesn't actually open a connection — perfect
    // for unit tests that just need a `PgPool` placeholder to
    // exercise pool-routing logic without a live Postgres.
    PgPoolOptions::new()
      .max_connections(1)
      .connect_lazy("postgres://test:test@localhost:5432/test")
      .expect("lazy pool")
  }

  #[tokio::test]
  async fn read_pool_falls_back_to_primary_when_not_configured() {
    let primary = lazy_pool();
    let db = Database {
      pool: primary.clone(),
      read_pool: None,
    };
    // Pointer equality (via the underlying Arc) — read_pool() must
    // hand back the *same* pool when no replica is set, not a
    // clone, so connection accounting stays per-pool.
    assert!(std::ptr::eq(db.read_pool(), &db.pool));
  }

  #[tokio::test]
  async fn read_pool_returns_replica_when_configured() {
    let primary = lazy_pool();
    let replica = lazy_pool();
    let db = Database {
      pool: primary,
      read_pool: Some(replica.clone()),
    };
    // Must point at the replica field, not the primary.
    assert!(std::ptr::eq(db.read_pool(), db.read_pool.as_ref().unwrap()));
    assert!(!std::ptr::eq(db.read_pool(), &db.pool));
  }
}
