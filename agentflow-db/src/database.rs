use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::info;

use crate::error::DbError;

/// Database connection manager.
#[derive(Clone, Debug)]
pub struct Database {
  pub pool: PgPool,
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

    Ok(Self { pool })
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

  /// Apply all pending migrations against the connected pool.
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
}
